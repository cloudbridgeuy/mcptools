use super::{registered_tools, to_dual_result, JsonRpcError, Tool, ToolKind};
use crate::sandbox;
use mcptools_core::sandbox::execute_output;

pub const DESCRIPTION: &str = r#"Runs JavaScript in a sandbox. Call find_tools first: each declaration it returns is an async global named after the tool, e.g. `const r = await jira_search({ jql: "..." })`. console.log lines return as logs, and the final expression or top-level return value as result. Only read tools are bound unless allowWrites or allowSpend is true."#;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteArgs {
    #[schemars(description = "JavaScript source to run in the sandbox.")]
    pub code: String,
    #[serde(rename = "allowWrites", default)]
    #[schemars(description = "Binds write tools in addition to read tools.")]
    pub allow_writes: bool,
    #[serde(rename = "allowSpend", default)]
    #[schemars(description = "Binds spend tools in addition to read tools.")]
    pub allow_spend: bool,
}

pub async fn handle_execute(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
    flags: super::ServeFlags,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ExecuteArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid params: {e}"),
            data: None,
        })?;
    let mut kinds = vec![ToolKind::Read];
    if args.allow_writes {
        kinds.push(ToolKind::Write);
    }
    if args.allow_spend {
        kinds.push(ToolKind::Spend);
    }
    let tools = registered_tools();
    let names = sandbox::bindings::bound_names(&tools, &kinds);
    let run = sandbox::run(
        &args.code,
        global.sandbox_limits(),
        sandbox::Bindings {
            names,
            global: global.clone(),
            flags,
        },
    )
    .await;
    let mut output = execute_output(run);
    if let Some(error) = output
        .error
        .as_mut()
        .filter(|error| error.name == "ReferenceError")
    {
        if let Some(message) = gated_reference_message(&error.message, &tools, &kinds) {
            error.message = message;
        }
    }
    let failed = output.error.is_some();
    let mut envelope = to_dual_result(&output)?;
    if failed {
        envelope["isError"] = serde_json::json!(true);
    }
    Ok(envelope)
}

fn gated_reference_message(message: &str, tools: &[Tool], allowed: &[ToolKind]) -> Option<String> {
    let name = message.strip_suffix(" is not defined")?;
    if name == "execute" {
        return None;
    }
    let kind = tools.iter().find(|tool| tool.name == name)?.kind;
    if allowed.contains(&kind) {
        return None;
    }
    match kind {
        ToolKind::Write => Some(format!(
            "{name} is a write tool; pass allowWrites: true to execute"
        )),
        ToolKind::Spend => Some(format!(
            "{name} is a spend tool; pass allowSpend: true to execute"
        )),
        ToolKind::Read => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::{handle_tools_call, listed_tools};
    use serde_json::json;

    fn global_with(extra: &[&str]) -> crate::Global {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Args {
            #[command(flatten)]
            global: crate::Global,
        }
        let mut argv = vec!["mcptools"];
        argv.extend_from_slice(extra);
        Args::parse_from(argv).global
    }

    fn clear_offline_env() {
        let keys: Vec<String> = std::env::vars()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("ATLASSIAN_") || key.starts_with("JIRA_"))
            .collect();
        for key in keys {
            std::env::remove_var(key);
        }
        std::env::remove_var("JEV_PROVIDER");
    }

    async fn call(
        name: &str,
        arguments: serde_json::Value,
        global: &crate::Global,
    ) -> Result<serde_json::Value, JsonRpcError> {
        handle_tools_call(
            Some(json!({"name": name, "arguments": arguments})),
            global,
            serve_flags(),
        )
        .await
    }

    fn serve_flags() -> crate::mcp::ServeFlags {
        crate::mcp::ServeFlags {
            discovery: false,
            code_mode: false,
        }
    }

    const KIND_PROBE: &str =
        "return [typeof jira_search, typeof jira_create, typeof images_generate, typeof execute]";

    #[tokio::test(flavor = "multi_thread")]
    async fn find_tools_then_execute_reaches_unlisted_tool() {
        clear_offline_env();
        let global = global_with(&[]);
        let found = call("find_tools", json!({"task": "search jira issues"}), &global)
            .await
            .unwrap();
        let names: Vec<&str> = found["structuredContent"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"jira_search"));
        assert!(!listed_tools(registered_tools(), true)
            .iter()
            .any(|tool| tool.name == "jira_search"));
        let executed = call(
            "execute",
            json!({"code": "try { await jira_search({ jql: \"x\" }); return null } catch (e) { return e.message }"}),
            &global,
        )
        .await
        .unwrap();
        let result = executed["structuredContent"]["result"].clone();
        let err = handle_tools_call(
            Some(json!({"name": "jira_search", "arguments": {"jql": "x"}})),
            &global,
            serve_flags(),
        )
        .await
        .unwrap_err();
        assert_eq!(result, json!(err.message));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn default_binds_read_only() {
        let global = global_with(&[]);
        let executed = call("execute", json!({"code": KIND_PROBE}), &global)
            .await
            .unwrap();
        assert_eq!(
            executed["structuredContent"]["result"],
            json!(["function", "undefined", "undefined", "undefined"])
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn allow_writes_binds_write_not_spend() {
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": KIND_PROBE, "allowWrites": true}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            executed["structuredContent"]["result"],
            json!(["function", "function", "undefined", "undefined"])
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn allow_spend_binds_spend() {
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": KIND_PROBE, "allowSpend": true}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            executed["structuredContent"]["result"],
            json!(["function", "undefined", "function", "undefined"])
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn success_returns_logs_and_result() {
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": "console.log(1); return {n: 2}"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            executed["structuredContent"],
            json!({"logs": ["1"], "result": {"n": 2}, "error": null})
        );
        assert!(executed.get("isError").is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn throw_returns_error_and_prior_logs() {
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": "console.log('a'); throw new TypeError('b')"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(executed["structuredContent"]["logs"], json!(["a"]));
        assert_eq!(executed["structuredContent"]["result"], json!(null));
        assert_eq!(
            executed["structuredContent"]["error"],
            json!({"message": "b", "name": "TypeError"})
        );
        assert_eq!(executed["isError"], json!(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn timeout_returns_timeout_error_and_prior_logs() {
        let global = global_with(&["--execute-timeout-secs", "1"]);
        let executed = call(
            "execute",
            json!({"code": "console.log('a'); while (true) {}"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(executed["structuredContent"]["logs"], json!(["a"]));
        assert_eq!(
            executed["structuredContent"]["error"]["name"],
            json!("TimeoutError")
        );
        assert_eq!(executed["isError"], json!(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn output_cap_keeps_lines_that_fit() {
        let global = global_with(&["--execute-output-kb", "1"]);
        let executed = call(
            "execute",
            json!({"code": "console.log('a'); console.log('x'.repeat(2000))"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(executed["structuredContent"]["logs"], json!(["a"]));
        assert_eq!(
            executed["structuredContent"]["error"]["name"],
            json!("OutputLimitError")
        );
        assert_eq!(executed["isError"], json!(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn memory_exhaustion_returns_internal_error() {
        let global = global_with(&["--execute-memory-mb", "1"]);
        let executed = call(
            "execute",
            json!({"code": "const a=[]; while(true) a.push('x'.repeat(1<<20))"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            executed["structuredContent"]["error"]["name"],
            json!("InternalError")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_code_is_invalid_params() {
        let global = global_with(&[]);
        let err = handle_tools_call(
            Some(json!({"name": "execute", "arguments": {}})),
            &global,
            serve_flags(),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn write_tool_names_allow_writes_flag() {
        clear_offline_env();
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": "console.log('a'); jira_update({})"}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(executed["structuredContent"]["logs"], json!(["a"]));
        assert_eq!(
            executed["structuredContent"]["error"],
            json!({
                "message": "jira_update is a write tool; pass allowWrites: true to execute",
                "name": "ReferenceError"
            })
        );
        assert_eq!(executed["structuredContent"]["result"], json!(null));
        assert_eq!(executed["isError"], json!(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn spend_tool_names_allow_spend_flag() {
        clear_offline_env();
        let global = global_with(&[]);
        let executed = call("execute", json!({"code": "images_generate({})"}), &global)
            .await
            .unwrap();
        assert_eq!(
            executed["structuredContent"]["error"],
            json!({
                "message": "images_generate is a spend tool; pass allowSpend: true to execute",
                "name": "ReferenceError"
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn allow_writes_does_not_cover_spend() {
        clear_offline_env();
        let global = global_with(&[]);
        let executed = call(
            "execute",
            json!({"code": "images_generate({})", "allowWrites": true}),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            executed["structuredContent"]["error"]["message"],
            json!("images_generate is a spend tool; pass allowSpend: true to execute")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn execute_reference_stays_plain() {
        clear_offline_env();
        let global = global_with(&[]);
        let executed = call("execute", json!({"code": "execute({})"}), &global)
            .await
            .unwrap();
        assert_eq!(
            executed["structuredContent"]["error"]["message"],
            json!("execute is not defined")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unknown_global_stays_plain() {
        clear_offline_env();
        let global = global_with(&[]);
        let executed = call("execute", json!({"code": "no_such_tool_175({})"}), &global)
            .await
            .unwrap();
        assert_eq!(
            executed["structuredContent"]["error"]["message"],
            json!("no_such_tool_175 is not defined")
        );
    }

    #[test]
    fn gated_reference_message_none_branches() {
        let tools = registered_tools();
        let writes = [ToolKind::Read, ToolKind::Write];
        assert_eq!(
            gated_reference_message("jira_update is not defined", &tools, &writes),
            None
        );
        assert_eq!(
            gated_reference_message("jira_update could not run", &tools, &writes),
            None
        );
        assert_eq!(
            gated_reference_message("execute is not defined", &tools, &writes),
            None
        );
    }
}
