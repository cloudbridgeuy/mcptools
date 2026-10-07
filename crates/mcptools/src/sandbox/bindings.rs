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
    match ctx.json_stringify(value)? {
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

    #[tokio::test(flavor = "multi_thread")]
    async fn read_only_set_has_forty_six_names_and_no_writes() {
        let read = bound_names(&registered_tools(), &[ToolKind::Read]);
        assert_eq!(read.len(), 46);
        assert!(read.contains(&"lane_list".to_string()));
        assert!(read.contains(&"lane_cleanup_plan".to_string()));
        assert!(!read.contains(&"lane_create".to_string()));
        assert!(read.contains(&"find_tools".to_string()));
        assert!(read.contains(&"jira_search".to_string()));
        assert!(read.contains(&"linear_issue_graph".to_string()));
        assert!(!read.contains(&"jira_create".to_string()));
        assert!(!read.contains(&"linear_project_create".to_string()));
        assert!(!read.contains(&"bitbucket_pr_comment_add".to_string()));
        assert!(!read.contains(&"images_generate".to_string()));
        let all = bound_names(
            &registered_tools(),
            &[ToolKind::Read, ToolKind::Write, ToolKind::Spend],
        );
        assert_eq!(all.len(), 69);
        assert!(all.contains(&"linear_project_create".to_string()));
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
