use mcptools_core::sandbox::{Limits, LogBuffer, Output, SandboxError};
use rquickjs::context::EvalOptions;
use rquickjs::function::Func;
use rquickjs::{AsyncContext, AsyncRuntime, Coerced, Ctx, Exception, FromJs, Promise, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

pub const PRELUDE: &str = r#"
globalThis.console = {
    log: (...args) => __log(__fmt(args)),
    info: (...args) => __log(__fmt(args)),
    warn: (...args) => __log(__fmt(args)),
    error: (...args) => __log(__fmt(args)),
    debug: (...args) => __log(__fmt(args)),
};
function __fmt(args) {
    return args.map(a => {
        if (typeof a === 'string') return a;
        if (a instanceof Error) return String(a);
        return JSON.stringify(a);
    }).join(' ');
}
"#;

pub async fn run(code: &str, limits: Limits) -> Result<Output, SandboxError> {
    let code = code.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(run_inner(code, limits))
    })
    .await;
    match outcome {
        Ok(result) => result,
        Err(err) => Err(message_error(err.to_string())),
    }
}

async fn run_inner(code: String, limits: Limits) -> Result<Output, SandboxError> {
    let logs = Rc::new(RefCell::new(LogBuffer::new(limits.output_bytes)));
    let stop: Rc<Cell<Option<StopReason>>> = Rc::new(Cell::new(None));
    let result = {
        let runtime = AsyncRuntime::new().map_err(js_error)?;
        runtime.set_memory_limit(limits.memory_bytes).await;
        let deadline = Instant::now() + limits.timeout;
        let handler_stop = stop.clone();
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                if handler_stop.get().is_some() {
                    return true;
                }
                if Instant::now() >= deadline {
                    handler_stop.set(Some(StopReason::Timeout));
                    return true;
                }
                false
            })))
            .await;
        let drive = eval(&runtime, &logs, &stop, code);
        let outcome = tokio::time::timeout(limits.timeout, drive).await;
        if let Some(reason) = stop.get() {
            return Err(reason.to_error(&limits));
        }
        match outcome {
            Ok(Ok(value)) => value,
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(SandboxError::Timeout(limits.timeout)),
        }
    };
    let lines = Rc::try_unwrap(logs)
        .expect("context released")
        .into_inner()
        .into_lines();
    let output = Output {
        logs: lines,
        result,
    };
    output.check_size(limits.output_bytes)?;
    Ok(output)
}

#[derive(Clone, Copy)]
enum StopReason {
    Timeout,
    OutputLimit,
}

impl StopReason {
    fn to_error(self, limits: &Limits) -> SandboxError {
        match self {
            Self::Timeout => SandboxError::Timeout(limits.timeout),
            Self::OutputLimit => SandboxError::OutputLimit(limits.output_bytes),
        }
    }
}

async fn eval(
    runtime: &AsyncRuntime,
    logs: &Rc<RefCell<LogBuffer>>,
    stop: &Rc<Cell<Option<StopReason>>>,
    code: String,
) -> Result<serde_json::Value, SandboxError> {
    let context = AsyncContext::full(runtime).await.map_err(js_error)?;
    let buffer = logs.clone();
    let stop = stop.clone();
    context
        .with(move |ctx| -> Result<(), SandboxError> {
            let log = Func::from(move |ctx: Ctx, line: String| -> rquickjs::Result<()> {
                let pushed = buffer.borrow_mut().push(line);
                if pushed.is_err() {
                    stop.set(Some(StopReason::OutputLimit));
                    return Err(Exception::throw_range(&ctx, "output limit exceeded"));
                }
                Ok(())
            });
            ctx.globals()
                .set("__log", log)
                .map_err(|err| eval_error(&ctx, err))?;
            ctx.eval::<(), _>(PRELUDE)
                .map_err(|err| eval_error(&ctx, err))?;
            Ok(())
        })
        .await?;
    context
        .async_with(async move |ctx| {
            let mut options = EvalOptions::default();
            options.promise = true;
            let promise: Promise = ctx
                .eval_with_options(code, options)
                .map_err(|err| eval_error(&ctx, err))?;
            let wrapper: Value = promise
                .into_future()
                .await
                .map_err(|err| eval_error(&ctx, err))?;
            let object = wrapper
                .into_object()
                .ok_or_else(|| message_error("completion value is not an object".to_string()))?;
            let value: Value = object.get("value").map_err(|err| eval_error(&ctx, err))?;
            let value = match value.clone().into_promise() {
                Some(promise) => promise
                    .into_future::<Value>()
                    .await
                    .map_err(|err| eval_error(&ctx, err))?,
                None => value,
            };
            let json = ctx
                .json_stringify(value)
                .map_err(|err| eval_error(&ctx, err))?;
            match json {
                None => Ok(serde_json::Value::Null),
                Some(text) => {
                    let text = text.to_string().map_err(js_error)?;
                    serde_json::from_str(&text).map_err(|err| message_error(err.to_string()))
                }
            }
        })
        .await
}

fn message_error(message: String) -> SandboxError {
    SandboxError::Js {
        name: "Error".to_string(),
        message,
    }
}

fn js_error(err: rquickjs::Error) -> SandboxError {
    message_error(err.to_string())
}

fn eval_error(ctx: &Ctx, err: rquickjs::Error) -> SandboxError {
    match err {
        rquickjs::Error::Exception => caught_error(ctx),
        other => js_error(other),
    }
}

fn caught_error(ctx: &Ctx) -> SandboxError {
    let value = ctx.catch();
    let object = value.clone().into_object();
    let name = object
        .as_ref()
        .and_then(|obj| obj.get::<_, String>("name").ok())
        .unwrap_or_else(|| "Error".to_string());
    let message = object
        .and_then(|obj| obj.get::<_, String>("message").ok())
        .unwrap_or_else(|| coerced_string(ctx, &value));
    SandboxError::Js { name, message }
}

fn coerced_string<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> String {
    Coerced::<String>::from_js(ctx, value.clone())
        .map(|coerced| coerced.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn small_limits(timeout: Duration) -> Limits {
        Limits {
            timeout,
            memory_bytes: 1024 * 1024,
            output_bytes: 4096,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn plain_expression_yields_its_value() {
        let output = run("1 + 1", Limits::default()).await.unwrap();
        assert_eq!(output.result, serde_json::json!(2));
        assert!(output.logs.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn console_logs_are_captured() {
        let output = run("console.log('a'); 'b'", Limits::default())
            .await
            .unwrap();
        assert_eq!(output.logs, vec!["a".to_string()]);
        assert_eq!(output.result, serde_json::json!("b"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn top_level_await_resolves() {
        let output = run("await Promise.resolve(3)", Limits::default())
            .await
            .unwrap();
        assert_eq!(output.result, serde_json::json!(3));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn thrown_type_error_is_mapped() {
        let error = run("throw new TypeError('x')", Limits::default())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            SandboxError::Js {
                name: "TypeError".to_string(),
                message: "x".to_string(),
            }
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn host_apis_are_unavailable() {
        for code in [
            "fetch('http://example.com')",
            "require('fs')",
            "process.env",
            "import('fs')",
            "import('std')",
        ] {
            let error = run(code, Limits::default()).await.unwrap_err();
            assert!(
                matches!(error, SandboxError::Js { .. }),
                "{code} => {error:?}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fresh_runtime_per_call() {
        let first = run("globalThis.x = 1", Limits::default()).await.unwrap();
        assert_eq!(first.result, serde_json::json!(1));
        let second = run("typeof x", Limits::default()).await.unwrap();
        assert_eq!(second.result, serde_json::json!("undefined"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tight_loop_stops_at_the_deadline() {
        let started = Instant::now();
        let limits = small_limits(Duration::from_millis(100));
        let error = run("while(true){}", limits).await.unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(100)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn try_catch_cannot_swallow_the_deadline() {
        let limits = small_limits(Duration::from_millis(100));
        let error = run("try { while(true){} } catch(e) {} 'x'", limits)
            .await
            .unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(100)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn yielding_loop_stops_at_the_deadline() {
        let limits = small_limits(Duration::from_millis(100));
        let error = run("while(true){ await Promise.resolve() }", limits)
            .await
            .unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(100)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn never_resolving_promise_stops_at_the_deadline() {
        let limits = small_limits(Duration::from_millis(100));
        let error = run("await new Promise(()=>{})", limits).await.unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(100)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn catastrophic_backtracking_stops_at_the_deadline() {
        let limits = small_limits(Duration::from_millis(100));
        let error = run("/(a+)+$/.test('a'.repeat(40)+'!')", limits)
            .await
            .unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(100)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn log_flood_hits_output_limit_despite_try_catch() {
        let limits = small_limits(Duration::from_millis(100));
        let code = "try { for(;;) console.log('x'.repeat(1000)) } catch(e) {} 'x'";
        let error = run(code, limits).await.unwrap_err();
        assert_eq!(error, SandboxError::OutputLimit(4096));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn oversized_result_hits_output_limit() {
        let limits = small_limits(Duration::from_millis(100));
        let error = run("'x'.repeat(5000)", limits).await.unwrap_err();
        assert_eq!(error, SandboxError::OutputLimit(4096));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn log_at_cap_with_empty_result_succeeds() {
        let limits = small_limits(Duration::from_millis(100));
        let output = run("console.log('x'.repeat(4094)); ''", limits)
            .await
            .unwrap();
        assert_eq!(output.logs, vec!["x".repeat(4094)]);
        assert_eq!(output.result, serde_json::json!(""));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn memory_exhaustion_is_recoverable() {
        let limits = small_limits(Duration::from_millis(100));
        let code = "const a=[]; while(true) a.push('x'.repeat(1<<20))";
        let error = run(code, limits).await.unwrap_err();
        assert!(
            matches!(&error, SandboxError::Js { name, .. } if name == "InternalError"),
            "{error:?}"
        );
        let output = run("1+1", small_limits(Duration::from_millis(100)))
            .await
            .unwrap();
        assert_eq!(output.result, serde_json::json!(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn busy_loop_does_not_stop_the_ticker() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let ticker = {
            let ticks = ticks.clone();
            tokio::spawn(async move {
                for _ in 0..20 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    ticks.fetch_add(1, Ordering::SeqCst);
                }
            })
        };
        let limits = small_limits(Duration::from_millis(300));
        let error = run("while(true){}", limits).await.unwrap_err();
        assert_eq!(error, SandboxError::Timeout(Duration::from_millis(300)));
        let count = ticks.load(Ordering::SeqCst);
        assert!(count >= 15, "ticks = {count}");
        ticker.await.unwrap();
    }
}
