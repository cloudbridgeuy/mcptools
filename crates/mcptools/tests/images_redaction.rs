#[allow(dead_code)]
mod common;

use serde_json::json;
use std::process::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "bearerPrefix123456789bearerSuffix";
const MESSAGE: &str = "Incorrect API key provided: bearerPrefix123456789bearerSuffix; echo bearerPrefix****bearerSuffix; other sk-proj-otherTestKey123. Check credentials.";
const REDACTED: &str =
    "Incorrect API key provided: [REDACTED]; echo [REDACTED]; other [REDACTED]. Check credentials.";

async fn rejects_secrets(command: &str, mcp: bool) {
    let stub = MockServer::start().await;
    let endpoint = if command == "generate" {
        "/images/generations"
    } else {
        "/images/edits"
    };
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("input.png");
    std::fs::write(&image, b"image").unwrap();
    let uri = stub.uri();
    let mut server =
        mcp.then(|| common::spawn_server(&[("MCPTOOLS_CONFIG", "/dev/null")]).unwrap());
    for status in [400, 401, 403, 429, 500] {
        stub.reset().await;
        Mock::given(method("POST"))
            .and(path(endpoint))
            .and(header("authorization", format!("Bearer {KEY}")))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(json!({"error": {"message": MESSAGE}})),
            )
            .expect(1)
            .mount(&stub)
            .await;
        let output = if let Some(server) = server.as_mut() {
            let response = server
                .request(json!({
                    "jsonrpc": "2.0", "id": status, "method": "tools/call",
                    "params": {"name": format!("images_{command}"), "arguments": {
                        "prompt": "otter", "images": [image], "api": "openai",
                        "apiKey": KEY, "baseUrl": uri, "outputDir": dir.path(), "allowSpend": true
                    }}
                }))
                .await;
            assert!(response.get("error").is_some(), "{response}");
            response.to_string()
        } else {
            let mut cli = Command::new(env!("CARGO_BIN_EXE_mcptools"));
            cli.args(["images", command]);
            if command != "vary" {
                cli.arg("otter");
            }
            if command != "generate" {
                cli.arg("--image").arg(&image);
            }
            let output = cli
                .args(["--api", "openai", "--base-url", &uri])
                .arg("--output-dir")
                .arg(dir.path())
                .env("MCPTOOLS_CONFIG", "/dev/null")
                .env("OPENAI_API_KEY", KEY)
                .output()
                .unwrap();
            assert!(!output.status.success());
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        };
        stub.verify().await;
        assert!(!output.contains(KEY), "{output}");
        assert!(!output.contains("bearerPrefix****bearerSuffix"), "{output}");
        assert!(!output.contains("sk-proj-otherTestKey123"), "{output}");
        assert!(output.contains(REDACTED), "{output}");
        assert!(output.contains(&format!("HTTP {status}")), "{output}");
    }
}

#[tokio::test]
async fn cli_generate_redacts_upstream_errors() {
    rejects_secrets("generate", false).await;
}

#[tokio::test]
async fn cli_edit_redacts_upstream_errors() {
    rejects_secrets("edit", false).await;
}

#[tokio::test]
async fn cli_vary_redacts_upstream_errors() {
    rejects_secrets("vary", false).await;
}

#[tokio::test]
async fn mcp_generate_redacts_upstream_errors() {
    rejects_secrets("generate", true).await;
}

#[tokio::test]
async fn mcp_edit_redacts_upstream_errors() {
    rejects_secrets("edit", true).await;
}

#[tokio::test]
async fn mcp_vary_redacts_upstream_errors() {
    rejects_secrets("vary", true).await;
}
