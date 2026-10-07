use super::{
    handle_tools_call, is_meta_tool, registered_tools, JsonRpcError, ServeFlags, ToolKind,
};

pub const DESCRIPTION: &str = r#"Calls one catalog tool by name and returns its result unchanged, e.g. `await tools.mcptools.call_tool({ name: "linear_issue_list", input: { team: "GUZ" } })`. Use find_tools to look up names and input shapes. Write tools need allowWrites: true, spend tools need allowSpend: true."#;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CallToolArgs {
    #[schemars(description = "Name of the catalog tool to call.")]
    pub name: String,
    #[serde(default)]
    #[schemars(description = "Arguments object passed to the named tool.")]
    pub input: Option<serde_json::Value>,
    #[serde(rename = "allowWrites", default)]
    #[schemars(description = "Permits calling a write tool.")]
    pub allow_writes: bool,
    #[serde(rename = "allowSpend", default)]
    #[schemars(description = "Permits calling a spend tool.")]
    pub allow_spend: bool,
}

fn invalid(message: String) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message,
        data: None,
    }
}

fn denied(message: String) -> serde_json::Value {
    serde_json::json!({
        "content": [{"type": "text", "text": message}],
        "isError": true
    })
}

pub async fn handle_call_tool(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
    flags: ServeFlags,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: CallToolArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| invalid(format!("Invalid params: {e}")))?;
    if is_meta_tool(&args.name) {
        return Err(invalid(format!(
            "{} cannot be called through call_tool",
            args.name
        )));
    }
    let kind = registered_tools()
        .iter()
        .find(|tool| tool.name == args.name)
        .map(|tool| tool.kind)
        .ok_or_else(|| invalid(format!("Unknown tool: {}", args.name)))?;
    match kind {
        ToolKind::Write if !args.allow_writes => {
            return Ok(denied(format!(
                "{} is a write tool; pass allowWrites: true to call_tool",
                args.name
            )))
        }
        ToolKind::Spend if !args.allow_spend => {
            return Ok(denied(format!(
                "{} is a spend tool; pass allowSpend: true to call_tool",
                args.name
            )))
        }
        _ => {}
    }
    Box::pin(handle_tools_call(
        Some(serde_json::json!({"name": args.name, "arguments": args.input})),
        global,
        flags,
    ))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::listed_tools;
    use serde_json::json;

    fn global() -> crate::Global {
        crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        }
    }

    fn code_mode() -> ServeFlags {
        ServeFlags {
            discovery: false,
            code_mode: true,
        }
    }

    async fn call(arguments: serde_json::Value) -> Result<serde_json::Value, JsonRpcError> {
        handle_call_tool(Some(arguments), &global(), code_mode()).await
    }

    #[test]
    fn code_mode_lists_find_tools_execute_call_tool() {
        let names: Vec<String> = listed_tools(registered_tools(), code_mode())
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names, ["find_tools", "execute", "call_tool"]);
    }

    #[tokio::test]
    async fn read_tool_dispatches() {
        let error = call(json!({"name": "hn_read_item", "input": {}}))
            .await
            .unwrap_err();
        assert!(
            error.message.starts_with("Invalid arguments"),
            "{}",
            error.message
        );
    }

    #[tokio::test]
    async fn cleanup_plan_dispatches_without_write_or_spend_permission() {
        let error = call(json!({"name":"lane_cleanup_plan", "input":{}}))
            .await
            .unwrap_err();
        assert_eq!(error.code, -32602);
        assert!(error.message.starts_with("Invalid arguments"));
    }

    #[tokio::test]
    async fn write_tool_requires_allow_writes() {
        let result = call(json!({"name": "jira_create", "input": {}}))
            .await
            .unwrap();
        assert_eq!(result["isError"], json!(true));
        assert_eq!(
            result["content"][0]["text"],
            json!("jira_create is a write tool; pass allowWrites: true to call_tool")
        );
    }

    #[tokio::test]
    async fn lane_create_requires_write_permission_before_dispatch() {
        let denied = call(json!({"name":"lane_create", "input":{}}))
            .await
            .unwrap();
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
        assert_eq!(
            call(json!({"name":"lane_create", "input":{}, "allowWrites":true}))
                .await
                .unwrap_err()
                .code,
            -32602
        );
    }

    #[tokio::test]
    async fn meta_and_unknown_names_are_rejected() {
        for name in ["call_tool", "execute", "find_tools"] {
            assert_eq!(call(json!({"name": name})).await.unwrap_err().code, -32602);
        }
        let error = call(json!({"name": "nope_tool"})).await.unwrap_err();
        assert_eq!(error.message, "Unknown tool: nope_tool");
    }
}
