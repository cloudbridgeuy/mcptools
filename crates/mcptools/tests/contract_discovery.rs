use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn request(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn run(command: &mut Command, requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for req in requests {
        writeln!(stdin, "{req}").unwrap();
    }
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut out = Vec::new();
    for _ in requests {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        out.push(serde_json::from_str(&line).unwrap());
    }
    child.wait().unwrap();
    out
}

fn without_jev_env(command: &mut Command) -> &mut Command {
    command
        .env_remove("JEV_PROVIDER")
        .env_remove("JEV_ENDPOINT")
        .env_remove("JEV_MODEL")
        .env_remove("JEV_API_KEY")
        .env_remove("OPENCODE_API_KEY")
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("AI_GATEWAY_API_KEY")
        .env_remove("TYPESAFE_API_KEY")
}

#[test]
fn discovery_flag_lists_find_tools_and_call_tool_then_calls_real_tool() {
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(binary());
    command.args(["mcp", "stdio", "--discovery"]);
    without_jev_env(&mut command);
    command
        .env("HOME", home.path())
        .env_remove("MCPTOOLS_DISCOVERY");

    let responses = run(
        &mut command,
        &[
            request(1, "tools/list", serde_json::json!({})),
            request(
                2,
                "tools/call",
                serde_json::json!({"name": "find_tools", "arguments": {"task": "close GUZ-22"}}),
            ),
            request(
                3,
                "tools/call",
                serde_json::json!({"name": "jira_query_list", "arguments": {}}),
            ),
        ],
    );

    let tools = responses[0]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "find_tools");
    assert_eq!(tools[1]["name"], "call_tool");

    let found = &responses[1]["result"]["structuredContent"];
    let found_tools = found["tools"].as_array().unwrap();
    assert!(!found_tools.is_empty());
    for tool in found_tools {
        assert!(tool["declaration"]
            .as_str()
            .is_some_and(|s| s.contains("declare function ")));
    }
    assert_eq!(found["backend"], "local");

    assert!(responses[2].get("error").is_none());
    assert_eq!(
        responses[2]["result"]["structuredContent"]["queries"],
        serde_json::json!([])
    );
}

#[test]
fn discovery_env_var_alone_lists_find_tools_and_call_tool() {
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(binary());
    command.args(["mcp", "stdio"]);
    without_jev_env(&mut command);
    command
        .env("MCPTOOLS_DISCOVERY", "true")
        .env("HOME", home.path());

    let responses = run(
        &mut command,
        &[request(1, "tools/list", serde_json::json!({}))],
    );

    let tools = responses[0]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "find_tools");
    assert_eq!(tools[1]["name"], "call_tool");
}

#[test]
fn no_flag_no_env_lists_all_tools() {
    let mut command = Command::new(binary());
    command.args(["mcp", "stdio"]);
    without_jev_env(&mut command);
    command.env_remove("MCPTOOLS_DISCOVERY");

    let responses = run(
        &mut command,
        &[request(1, "tools/list", serde_json::json!({}))],
    );

    let tools = responses[0]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 71);
}

#[test]
fn lanes_are_discoverable_and_deny_unconfigured_access_in_all_modes() {
    for mode in [None, Some("--discovery"), Some("--code-mode")] {
        let mut command = Command::new(binary());
        command.args(["mcp", "stdio"]);
        if let Some(mode) = mode {
            command.arg(mode);
        }
        without_jev_env(&mut command);
        command
            .env_remove("MCPTOOLS_DISCOVERY")
            .env_remove("MCPTOOLS_CODE_MODE")
            .env_remove("MCPTOOLS_LANE_REPOS");
        let responses = run(
            &mut command,
            &[
                request(
                    1,
                    "tools/call",
                    serde_json::json!({"name":"find_tools", "arguments":{"domain":"lane"}}),
                ),
                request(
                    2,
                    "tools/call",
                    serde_json::json!({"name":"lane_list", "arguments":{"repo":"/"}}),
                ),
                request(
                    3,
                    "tools/call",
                    serde_json::json!({"name":"call_tool", "arguments":{"name":"lane_create", "input":{"repo":"/", "branch":"topic", "base":"main"}}}),
                ),
                request(
                    4,
                    "tools/call",
                    serde_json::json!({"name":"lane_cleanup_plan", "arguments":{"repo":"/"}}),
                ),
                request(
                    5,
                    "tools/call",
                    serde_json::json!({"name":"call_tool", "arguments":{"name":"lane_cleanup_plan", "input":{"repo":"/"}}}),
                ),
                request(
                    6,
                    "tools/call",
                    serde_json::json!({"name":"execute", "arguments":{"code":"return await lane_cleanup_plan({repo:'/'});"}}),
                ),
            ],
        );
        let found = responses[0]["result"]["structuredContent"]["tools"]
            .as_array()
            .unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0]["name"], "lane_list");
        let declaration = found[0]["declaration"].as_str().unwrap();
        assert!(declaration.contains("string | null"));
        assert!(declaration.contains("\"unknown\""));
        assert!(responses[1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("MCPTOOLS_LANE_REPOS"));
        assert_eq!(responses[2]["result"]["isError"], true);
        assert!(responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
        let plan = found
            .iter()
            .find(|tool| tool["name"] == "lane_cleanup_plan")
            .unwrap();
        assert!(plan["declaration"].as_str().unwrap().contains("laneIds"));
        for index in [3, 4] {
            assert!(responses[index]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("MCPTOOLS_LANE_REPOS"));
        }
        assert!(
            responses[5]["result"]["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("MCPTOOLS_LANE_REPOS")
        );
    }
}
