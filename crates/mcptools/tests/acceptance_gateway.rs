#![cfg(unix)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    dir: tempfile::TempDir,
    config: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new(read_only: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let log = dir.path().join("calls.jsonl");
        let fixture = Self { dir, config, log };
        fixture.configure(read_only, "fake");
        fixture
    }

    fn configure(&self, read_only: bool, prefix: &str) {
        std::fs::write(
            &self.config,
            format!("[[gateway.upstreams]]\nprefix = {prefix:?}\ncommand = '/bin/sh'\nargs = {}\nreadOnly = {}\n[gateway.upstreams.env]\nFIXTURE_LOG = {}\n", json!(["-c", "sleep 0.05; exec python3 \"$@\"", "gateway-fixture", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/upstream.py")]), json!(if read_only { vec!["echo"] } else { vec![] }), json!(self.log)),
        )
        .unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
        command.env("MCPTOOLS_CONFIG", &self.config);
        command.env_remove("JEV_PROVIDER");
        command
    }

    fn start(&self) -> Session {
        let mut child = self
            .command()
            .args(["mcp", "stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Session {
            child,
            input,
            output,
        }
    }

    fn events(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Session {
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        writeln!(
            self.input.as_mut().unwrap(),
            "{}",
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
        )
        .unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.rpc("tools/call", json!({"name": name, "arguments": arguments}))["result"].clone()
    }

    fn finish(mut self) {
        self.input.take();
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn discovery_cache_and_config_invalidation() {
    let fixture = Fixture::new(false);
    let first = fixture.command().args(["mcp", "catalog"]).output().unwrap();
    assert!(first.status.success());
    assert!(String::from_utf8_lossy(&first.stdout).contains("fake\tfake_echo\t"));
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|e| e["method"] == "tools/list")
            .count(),
        1
    );
    let second = fixture.command().args(["mcp", "catalog"]).output().unwrap();
    assert!(second.status.success());
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|e| e["method"] == "tools/list")
            .count(),
        1
    );
    assert!(std::fs::read_dir(fixture.dir.path()).unwrap().count() >= 3);
    fixture.configure(false, "changed");
    let changed = fixture.command().args(["mcp", "catalog"]).output().unwrap();
    assert!(changed.status.success());
    assert!(String::from_utf8_lossy(&changed.stdout).contains("changed_echo"));
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|e| e["method"] == "tools/list")
            .count(),
        2
    );
}

#[test]
fn prefixed_discovery_declarations_and_hidden_list() {
    let fixture = Fixture::new(false);
    let declarations = fixture
        .command()
        .args(["mcp", "declarations", "fake_echo"])
        .output()
        .unwrap();
    assert!(
        declarations.status.success(),
        "{}",
        String::from_utf8_lossy(&declarations.stderr)
    );
    assert!(String::from_utf8_lossy(&declarations.stdout).contains("declare function fake_echo"));
    let mut session = fixture.start();
    let found = session.call("find_tools", json!({"domain": "fake"}));
    assert_eq!(found["structuredContent"]["tools"][0]["name"], "fake_echo");
    let listed = session.rpc("tools/list", json!({}));
    assert!(!listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "fake_echo"));
    session.finish();
}

#[test]
fn forwarding_and_default_write_gate() {
    let fixture = Fixture::new(false);
    let mut session = fixture.start();
    let denied = session.call(
        "call_tool",
        json!({"name": "fake_echo", "input": {"value": "hi"}}),
    );
    assert_eq!(denied["isError"], true);
    assert!(denied["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("allowWrites"));
    assert!(!fixture.events().iter().any(|e| e["method"] == "tools/call"));
    let spend_only = session.call(
        "call_tool",
        json!({"name": "fake_echo", "input": {}, "allowSpend": true}),
    );
    assert_eq!(spend_only["isError"], true);
    let direct = session.call("fake_echo", json!({}));
    assert_eq!(direct["isError"], true);
    assert!(!fixture.events().iter().any(|e| e["method"] == "tools/call"));
    let result = session.call(
        "call_tool",
        json!({"name": "fake_echo", "input": {"value": "hi"}, "allowWrites": true}),
    );
    assert_eq!(
        result,
        json!({"content": [{"type": "text", "text": "opaque"}], "structuredContent": {"name": "echo", "arguments": {"value": "hi"}}, "_meta": {"untouched": true}})
    );
    let denied = session.call("execute", json!({"code": "return await fake_echo({})"}));
    assert!(denied["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("allowWrites"));
    let executed = session.call(
        "execute",
        json!({"code": "return await fake_echo({value: 'js'})", "allowWrites": true}),
    );
    assert_eq!(
        executed["structuredContent"]["result"]["arguments"]["value"],
        "js"
    );
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|e| e.get("pid").is_some())
            .count(),
        1
    );
    session.finish();
}

#[test]
fn read_only_override() {
    let fixture = Fixture::new(true);
    let warmed = fixture.command().args(["mcp", "catalog"]).output().unwrap();
    assert!(warmed.status.success());
    let mut session = fixture.start();
    let found = session.call("find_tools", json!({"domain": "fake"}));
    assert_eq!(found["structuredContent"]["tools"][0]["name"], "fake_echo");
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event.get("pid").is_some())
            .count(),
        1
    );
    let result = session.call(
        "call_tool",
        json!({"name": "fake_echo", "input": {"value": "read"}}),
    );
    assert_eq!(result["structuredContent"]["arguments"]["value"], "read");
    let result = session.call(
        "execute",
        json!({"code": "return await fake_echo({value: 'read-js'})"}),
    );
    assert_eq!(
        result["structuredContent"]["result"]["arguments"]["value"],
        "read-js"
    );
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event.get("pid").is_some())
            .count(),
        2
    );
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event["method"] == "tools/list")
            .count(),
        1
    );
    session.finish();
}

#[test]
fn punctuated_upstream_names_support_discovery_declarations_and_execute() {
    let fixture = Fixture::new(true);
    let content = std::fs::read_to_string(&fixture.config)
        .unwrap()
        .replace("\"echo\"", "\"resolve-library-id\"");
    std::fs::write(
        &fixture.config,
        format!("{content}FIXTURE_TOOL = 'resolve-library-id'\n"),
    )
    .unwrap();
    let mut session = fixture.start();
    let found = session.call("find_tools", json!({"domain": "fake"}));
    assert_eq!(
        found["structuredContent"]["tools"][0]["name"],
        "fake_resolve-library-id"
    );
    assert!(found["structuredContent"]["tools"][0]["declaration"]
        .as_str()
        .unwrap()
        .contains("\"fake_resolve-library-id\"(input:"));
    let executed = session.call(
        "execute",
        json!({"code": "return await globalThis['fake_resolve-library-id']({value: 'tokio'})"}),
    );
    assert_eq!(
        executed["structuredContent"]["result"]["arguments"]["value"],
        "tokio"
    );
    session.finish();
}

#[test]
fn upstream_stopped_on_exit_and_abrupt_owner_death() {
    for abrupt in [false, true, true, true, true] {
        let fixture = Fixture::new(true);
        let mut session = fixture.start();
        let result = session.call("call_tool", json!({"name": "fake_echo", "input": {}}));
        assert!(result.get("structuredContent").is_some());
        let events = fixture.events();
        let process = events
            .iter()
            .find(|event| event.get("pid").is_some())
            .unwrap();
        let pids = [
            process["pid"].as_u64().unwrap(),
            process["descendant"].as_u64().unwrap(),
        ];
        if abrupt {
            session.child.kill().unwrap();
            session.child.wait().unwrap();
        } else {
            session.finish();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        for pid in pids {
            loop {
                let status = Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "stat="])
                    .output()
                    .unwrap();
                if !status.status.success()
                    || String::from_utf8_lossy(&status.stdout)
                        .trim_start()
                        .starts_with('Z')
                {
                    break;
                }
                assert!(Instant::now() < deadline, "upstream {pid} orphaned");
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
