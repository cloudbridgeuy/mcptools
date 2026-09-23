use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const CLEARED_ENV: [&str; 10] = [
    "MCPTOOLS_DISCOVERY",
    "ATLASSIAN_BASE_URL",
    "JEV_PROVIDER",
    "JEV_ENDPOINT",
    "JEV_MODEL",
    "JEV_API_KEY",
    "OPENCODE_API_KEY",
    "OPENROUTER_API_KEY",
    "AI_GATEWAY_API_KEY",
    "TYPESAFE_API_KEY",
];

pub struct McpServer {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub fn spawn_server(env: &[(&str, &str)]) -> std::io::Result<McpServer> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
    command
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    for key in CLEARED_ENV {
        command.env_remove(key);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn()?;
    let input = child.stdin.take().expect("piped stdin");
    let output = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut server = McpServer {
        child,
        input,
        output,
    };
    server.send(&envelope("initialize", serde_json::json!({})))?;
    server.receive()?;
    server.send(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
    server.receive()?;
    Ok(server)
}

fn envelope(method: &str, params: serde_json::Value) -> serde_json::Value {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "method": method,
        "params": params,
    })
}

impl McpServer {
    fn send(&mut self, message: &serde_json::Value) -> std::io::Result<()> {
        writeln!(self.input, "{message}")
    }

    fn receive(&mut self) -> std::io::Result<serde_json::Value> {
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "server closed stdout before sending a response",
            ));
        }
        Ok(serde_json::from_str(line.trim())?)
    }

    pub async fn request(&mut self, req: serde_json::Value) -> serde_json::Value {
        self.send(&req).expect("write request to server stdin");
        self.receive().expect("read response from server stdout")
    }

    pub async fn tools_call(&mut self, name: &str, args: serde_json::Value) -> serde_json::Value {
        let response = self
            .request(envelope(
                "tools/call",
                serde_json::json!({"name": name, "arguments": args}),
            ))
            .await;
        response["result"].clone()
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
