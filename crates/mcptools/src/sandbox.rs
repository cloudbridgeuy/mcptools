use mcptools_core::sandbox::{Limits, LogBuffer, Output, SandboxError};
use rquickjs::context::EvalOptions;
use rquickjs::function::Func;
use rquickjs::{AsyncContext, AsyncRuntime, Coerced, Ctx, FromJs, Promise, Value};
use std::cell::RefCell;
use std::rc::Rc;

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
    let result = {
        let runtime = AsyncRuntime::new().map_err(js_error)?;
        runtime.set_memory_limit(limits.memory_bytes).await;
        eval(&runtime, &logs, code).await?
    };
    let lines = Rc::try_unwrap(logs)
        .expect("context released")
        .into_inner()
        .into_lines();
    Ok(Output {
        logs: lines,
        result,
    })
}

async fn eval(
    runtime: &AsyncRuntime,
    logs: &Rc<RefCell<LogBuffer>>,
    code: String,
) -> Result<serde_json::Value, SandboxError> {
    let context = AsyncContext::full(runtime).await.map_err(js_error)?;
    let buffer = logs.clone();
    context
        .with(move |ctx| {
            let log = Func::from(move |line: String| {
                let _ = buffer.borrow_mut().push(line);
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
}
