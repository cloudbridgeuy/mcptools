use crate::mcp::tools::handle_tools_call;
use crate::mcp::{JsonRpcError, Tool, ToolKind};
use crate::sandbox::Bindings;
use rquickjs::function::{Async, Opt};
use rquickjs::{Ctx, Exception, Function, Value};

pub fn bound_names(tools: &[Tool], allowed: &[ToolKind]) -> Vec<String> {
    tools
        .iter()
        .filter(|tool| tool.name != "execute" && tool.name != "call_tool")
        .filter(|tool| allowed.contains(&tool.kind))
        .map(|tool| tool.name.clone())
        .collect()
}

pub fn install<'js>(ctx: &Ctx<'js>, bindings: &Bindings) -> rquickjs::Result<()> {
    for name in &bindings.names {
        let bound = name.clone();
        let global = bindings.global.clone();
        let flags = bindings.flags;
        let function = Function::new(
            ctx.clone(),
            Async(move |ctx: Ctx<'js>, input: Opt<Value<'js>>| {
                let name = bound.clone();
                let global = global.clone();
                async move { call(ctx, name, global, flags, input.0).await }
            }),
        )?;
        ctx.globals().set(name, function)?;
    }
    Ok(())
}

async fn call<'js>(
    ctx: Ctx<'js>,
    name: String,
    global: crate::Global,
    flags: crate::mcp::ServeFlags,
    input: Option<Value<'js>>,
) -> rquickjs::Result<Value<'js>> {
    let arguments = arguments(&ctx, input)?;
    match handle_tools_call(
        Some(serde_json::json!({ "name": name, "arguments": arguments })),
        &global,
        flags,
    )
    .await
    {
        Ok(envelope) => {
            let value = mcptools_core::sandbox::tool_result(envelope);
            let text = serde_json::to_string(&value)
                .map_err(|err| Exception::throw_type(&ctx, &err.to_string()))?;
            ctx.json_parse(text)
        }
        Err(err) => Err(throw_rpc(&ctx, err)?),
    }
}

fn arguments<'js>(
    ctx: &Ctx<'js>,
    input: Option<Value<'js>>,
) -> rquickjs::Result<Option<serde_json::Value>> {
    let value = match input {
        None => return Ok(None),
        Some(value) if value.is_undefined() => return Ok(None),
        Some(value) => value,
    };
    let replacer = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, _key: String, value: Value<'js>| {
            if value.as_number().is_some_and(|number| !number.is_finite()) {
                return Err(Exception::throw_type(
                    &ctx,
                    "Tool arguments must not contain non-finite numbers",
                ));
            }
            Ok(value)
        },
    )?;
    match ctx.json_stringify_replacer(value, replacer)? {
        None => Ok(None),
        Some(text) => {
            let text = text.to_string()?;
            serde_json::from_str(&text)
                .map(Some)
                .map_err(|err| Exception::throw_type(ctx, &err.to_string()))
        }
    }
}

fn throw_rpc<'js>(ctx: &Ctx<'js>, err: JsonRpcError) -> rquickjs::Result<rquickjs::Error> {
    let exception = Exception::from_message(ctx.clone(), &err.message)?;
    exception.as_object().set("code", err.code)?;
    if let Some(data) = err.data {
        let text = serde_json::to_string(&data)
            .map_err(|cause| Exception::throw_type(ctx, &cause.to_string()))?;
        let value = ctx.json_parse(text)?;
        exception.as_object().set("data", value)?;
    }
    Ok(exception.throw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::registered_tools;
    use crate::sandbox::run;
    use clap::Parser;
    use mcptools_core::sandbox::Limits;
    use serde_json::json;

    fn test_global() -> crate::Global {
        #[derive(Parser)]
        struct Args {
            #[command(flatten)]
            global: crate::Global,
        }
        Args::parse_from(["mcptools"]).global
    }

    fn test_flags() -> crate::mcp::ServeFlags {
        crate::mcp::ServeFlags {
            discovery: false,
            code_mode: false,
        }
    }

    fn read_bindings() -> Bindings {
        Bindings {
            names: bound_names(&registered_tools(), &[ToolKind::Read]),
            global: test_global(),
            flags: test_flags(),
        }
    }

    fn clear_offline_env() {
        for key in [
            "ATLASSIAN_BASE_URL",
            "ATLASSIAN_EMAIL",
            "ATLASSIAN_API_TOKEN",
            "JIRA_BASE_URL",
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JEV_PROVIDER",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn arguments_reject_nonfinite_numbers_before_json_conversion() {
        let runtime = rquickjs::Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        context.with(|ctx| {
            for number in ["NaN", "Infinity", "-Infinity"] {
                for source in [
                    number.to_string(),
                    format!("({{nested: {{number: {number}}}}})"),
                    format!("[0, {{nested: [null, {number}]}}]"),
                ] {
                    let value = ctx.eval::<Value, _>(source).unwrap();
                    assert!(arguments(&ctx, Some(value)).is_err());
                    let error = ctx.catch().into_object().unwrap();
                    assert_eq!(error.get::<_, String>("name").unwrap(), "TypeError");
                    assert_eq!(
                        error.get::<_, String>("message").unwrap(),
                        "Tool arguments must not contain non-finite numbers"
                    );
                }
            }
        });
    }

    #[test]
    fn arguments_preserve_json_semantics_and_serialization_errors() {
        let runtime = rquickjs::Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        context.with(|ctx| {
            assert_eq!(arguments(&ctx, None).unwrap(), None);
            for source in [
                "undefined",
                "null",
                "-1.5",
                "({number: 1e308, nil: null, omitted: undefined, values: [0, -1.5, null, undefined]})",
                "({toJSON() { return {number: 2, nil: null}; }})",
            ] {
                let value = ctx.eval::<Value, _>(source).unwrap();
                let expected = ctx
                    .json_stringify(value.clone())
                    .unwrap()
                    .map(|text| serde_json::from_str::<serde_json::Value>(&text.to_string().unwrap()).unwrap());
                assert_eq!(arguments(&ctx, Some(value)).unwrap(), expected);
            }
            for source in [
                "(() => { const value = {}; value.self = value; return value; })()",
                "1n",
                "({get value() { throw new Error('getter failed'); }})",
            ] {
                let value = ctx.eval::<Value, _>(source).unwrap();
                assert!(ctx.json_stringify(value.clone()).is_err());
                let expected = ctx.catch().into_object().unwrap();
                assert!(arguments(&ctx, Some(value)).is_err());
                let actual = ctx.catch().into_object().unwrap();
                for field in ["name", "message"] {
                    assert_eq!(
                        actual.get::<_, String>(field).unwrap(),
                        expected.get::<_, String>(field).unwrap()
                    );
                }
            }
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn execute_rejects_nonfinite_milestone_order_before_dispatch() {
        for number in ["NaN", "Infinity", "-Infinity"] {
            let result = handle_tools_call(
                Some(json!({
                    "name": "execute",
                    "arguments": {
                        "code": format!("return await linear_project_milestone_create({{project: '12345678-1234-1234-1234-123456789abc', name: ' ', sortOrder: {number}}})"),
                        "allowWrites": true,
                    },
                })),
                &test_global(),
                test_flags(),
            )
            .await
            .unwrap();
            assert_eq!(result["isError"], true);
            assert_eq!(result["structuredContent"]["error"]["name"], "TypeError");
            assert_eq!(
                result["structuredContent"]["error"]["message"],
                "Tool arguments must not contain non-finite numbers"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn read_only_set_has_forty_nine_names_and_no_writes() {
        let read = bound_names(&registered_tools(), &[ToolKind::Read]);
        assert_eq!(read.len(), 49);
        assert!(read.contains(&"lane_list".to_string()));
        assert!(read.contains(&"lane_cleanup_plan".to_string()));
        assert!(!read.contains(&"lane_create".to_string()));
        assert!(read.contains(&"find_tools".to_string()));
        assert!(read.contains(&"jira_search".to_string()));
        assert!(read.contains(&"linear_issue_graph".to_string()));
        assert!(read.contains(&"linear_project_status_list".to_string()));
        assert!(read.contains(&"linear_project_milestone_list".to_string()));
        assert!(read.contains(&"linear_project_update_list".to_string()));
        assert!(!read.contains(&"jira_create".to_string()));
        assert!(!read.contains(&"linear_project_create".to_string()));
        assert!(!read.contains(&"linear_project_update".to_string()));
        assert!(!read.contains(&"linear_project_update_create".to_string()));
        assert!(!read.contains(&"bitbucket_pr_comment_add".to_string()));
        assert!(!read.contains(&"images_generate".to_string()));
        let all = bound_names(
            &registered_tools(),
            &[ToolKind::Read, ToolKind::Write, ToolKind::Spend],
        );
        assert_eq!(all.len(), 75);
        assert!(all.contains(&"linear_project_create".to_string()));
        assert!(all.contains(&"linear_project_update".to_string()));
        assert!(all.contains(&"linear_project_update_create".to_string()));
        assert!(all.contains(&"bitbucket_pr_comment_add".to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn excluded_tool_is_undefined() {
        let run = run(
            "return [typeof jira_create, typeof jira_search]",
            Limits::default(),
            read_bindings(),
        )
        .await;
        assert_eq!(run.outcome.unwrap(), json!(["undefined", "function"]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn success_returns_structured_content() {
        clear_offline_env();
        let first = run(
            "const r = await find_tools({ task: 'search jira issues' }); return Array.isArray(r.tools)",
            Limits::default(),
            read_bindings(),
        )
        .await;
        assert_eq!(first.outcome.unwrap(), json!(true));
        let second = run(
            "return 'content' in (await find_tools({ task: 'x' }))",
            Limits::default(),
            read_bindings(),
        )
        .await;
        assert_eq!(second.outcome.unwrap(), json!(false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn error_rejects_with_code_and_message() {
        clear_offline_env();
        let global = test_global();
        let bindings = Bindings {
            names: bound_names(&registered_tools(), &[ToolKind::Read]),
            global: global.clone(),
            flags: test_flags(),
        };
        let run = run(
            "try { await jira_search({ jql: 'x' }); return null } catch (e) { return [e.code, e.message, e instanceof Error, e.name] }",
            Limits::default(),
            bindings,
        )
        .await;
        let result = run.outcome.unwrap();
        let err = handle_tools_call(
            Some(json!({"name": "jira_search", "arguments": {"jql": "x"}})),
            &global,
            test_flags(),
        )
        .await
        .unwrap_err();
        assert_eq!(result[0].as_i64(), Some(err.code as i64));
        assert_eq!(result[1].as_str(), Some(err.message.as_str()));
        assert_eq!(result[2], json!(true));
        assert_eq!(result[3], json!("Error"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_arguments_reject_with_invalid_params() {
        let run = run(
            "try { await pdf_info(); return null } catch (e) { return e.code }",
            Limits::default(),
            read_bindings(),
        )
        .await;
        assert_eq!(run.outcome.unwrap(), json!(-32602));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_calls_resolve() {
        clear_offline_env();
        let run = run(
            "const r = await Promise.all([find_tools({task:'a'}), find_tools({task:'b'})]); return r.length",
            Limits::default(),
            read_bindings(),
        )
        .await;
        assert_eq!(run.outcome.unwrap(), json!(2));
    }
}
