mod common;

use serde_json::json;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_stdin_default_path_is_deterministic_and_matches_file_input() {
    let directory = tempfile::tempdir().unwrap();
    let markdown = include_str!("../../core/tests/fixtures/render.md");
    let mut child = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .args(["md", "render", "-", "--title", "Offline <report>", "--json"])
        .env("XDG_DATA_HOME", directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(markdown.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let path = std::path::Path::new(result["path"].as_str().unwrap());
    assert!(path.starts_with(
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("mcptools/renders")
    ));
    assert!(path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("offline-report-"));
    assert_eq!(
        reqwest::Url::from_file_path(path).unwrap().as_str(),
        result["url"]
    );
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<title>Offline &lt;report&gt;</title>"));
    assert!(html.len() > 2_748_992);
    assert!(html.contains("securityLevel: 'strict'"));
    let input = directory.path().join("input.md");
    std::fs::write(&input, markdown).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .args(["md", "render"])
        .arg(input)
        .args(["--title", "Offline <report>"])
        .env("XDG_DATA_HOME", directory.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        path.to_str().unwrap()
    );
    assert_eq!(html, std::fs::read_to_string(path).unwrap());
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
}

#[tokio::test]
async fn mcp_requires_writes_and_replaces_output_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("space #.html");
    std::fs::write(&path, "old contents").unwrap();
    let mut server = common::spawn_server(&[]).unwrap();
    for input in [
        json!({"markdown":"# Denied", "path":path}),
        json!({"markdown":"# Denied", "path":path, "allowWrites":false}),
    ] {
        let denied = server.tools_call("md_render", input).await;
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old contents");
    }
    let result = server
        .tools_call(
            "md_render",
            json!({
                "markdown":"# Allowed", "path":path, "title":"MCP", "allowWrites":true
            }),
        )
        .await;
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["path"], path.to_str().unwrap());
    assert_eq!(
        result["structuredContent"]["url"],
        reqwest::Url::from_file_path(&path).unwrap().as_str()
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(result["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        result["structuredContent"]
    );
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("<h1 id=\"allowed\">Allowed</h1>"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
