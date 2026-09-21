use super::JsonRpcError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FoundTool {
    pub name: String,
    pub domain: String,
    pub score: f64,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Jev,
    Local,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FoundTools {
    pub none: f64,
    pub tools: Vec<FoundTool>,
    pub backend: Backend,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindToolsArgs {
    #[schemars(description = "Natural-language description of the task")]
    pub task: String,
    #[schemars(description = "Maximum number of tools to return (default 5)")]
    pub k: Option<usize>,
}

fn attach_schemas(ranking: mcptools_core::find_tools::Ranking, backend: Backend) -> FoundTools {
    let registered = super::registered_tools();
    let tools: Vec<FoundTool> = ranking
        .tools
        .into_iter()
        .filter_map(|r| {
            registered
                .iter()
                .find(|t| t.name == r.name)
                .map(|t| FoundTool {
                    name: r.name,
                    domain: r.domain,
                    score: r.score,
                    input_schema: t.input_schema.as_object().cloned().unwrap_or_default(),
                })
        })
        .collect();
    FoundTools {
        none: ranking.none,
        tools,
        backend,
    }
}

pub async fn find_tools_with(
    config: Result<Option<mcptools_core::jev::GatewayConfig>, mcptools_core::jev::ConfigError>,
    task: &str,
    k: usize,
) -> Result<FoundTools, mcptools_core::find_tools::FindToolsError> {
    let catalog = super::tool_catalog();
    let local = mcptools_core::find_tools::rank_tools(task, &catalog, k)?;
    let cfg = match config {
        Ok(Some(c)) => c,
        _ => return Ok(attach_schemas(local, Backend::Local)),
    };
    let body = mcptools_core::jev::build_request(task, &catalog, &cfg.model);
    match crate::jev::classify(&cfg, &body).await {
        Ok(text) => match mcptools_core::jev::parse_ranking(&text, &catalog) {
            Ok(r) => {
                let sel = mcptools_core::jev::select(r, k);
                Ok(attach_schemas(sel, Backend::Jev))
            }
            Err(_) => Ok(attach_schemas(local, Backend::Local)),
        },
        Err(_) => Ok(attach_schemas(local, Backend::Local)),
    }
}

pub async fn find_tools(
    task: &str,
    k: usize,
) -> Result<FoundTools, mcptools_core::find_tools::FindToolsError> {
    find_tools_with(
        mcptools_core::jev::gateway_config(|key| std::env::var(key).ok()),
        task,
        k,
    )
    .await
}

pub async fn handle_find_tools(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: FindToolsArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;
    let result = find_tools(
        &args.task,
        args.k.unwrap_or(mcptools_core::find_tools::DEFAULT_K),
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32602,
        message: e.to_string(),
        data: None,
    })?;
    super::to_dual_result(result)
}

#[cfg(test)]
mod find_tools_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn parse_golden() -> Vec<(String, Option<Vec<String>>, String)> {
        let content = include_str!("fixtures/golden-queries.tsv");
        let mut rows = vec![];
        for (i, line) in content.lines().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 {
                continue;
            }
            let task = parts[0].to_string();
            let expected = if parts[1] == "NONE" {
                None
            } else {
                Some(parts[1].split(',').map(|s| s.trim().to_string()).collect())
            };
            let kind = parts[2].to_string();
            rows.push((task, expected, kind));
        }
        rows
    }

    #[tokio::test]
    async fn find_tools_attaches_real_input_schema() {
        let result = find_tools("close GUZ-22", 5).await.unwrap();
        let registered = super::super::registered_tools();
        for found in &result.tools {
            let reg = registered.iter().find(|t| t.name == found.name).unwrap();
            let reg_schema = reg.input_schema.as_object().cloned().unwrap_or_default();
            assert_eq!(found.input_schema, reg_schema);
        }
    }

    #[tokio::test]
    async fn golden_recall_at_5_meets_threshold() {
        let rows = parse_golden();
        let in_scope: Vec<_> = rows.iter().filter(|(_, _, k)| k == "in_scope").collect();
        let mut hits = 0;
        for (task, expected, _) in &in_scope {
            if let Some(exps) = expected {
                let res = find_tools(task, 5).await.unwrap();
                let names: Vec<_> = res.tools.iter().map(|t| t.name.as_str()).collect();
                if exps.iter().any(|e| names.contains(&e.as_str())) {
                    hits += 1;
                }
            }
        }
        let recall = hits as f64 / in_scope.len() as f64;
        assert!(recall >= 0.85, "recall@5 = {}", recall);
    }

    #[tokio::test]
    async fn close_guz_22_ranks_linear_issue_update_in_top_5() {
        let res = find_tools("close GUZ-22", 5).await.unwrap();
        assert!(res.tools.iter().any(|t| t.name == "linear_issue_update"));
    }

    #[tokio::test]
    async fn unrelated_rows_score_higher_none_than_close_guz_22() {
        let rows = parse_golden();
        let unrelated: Vec<_> = rows.iter().filter(|(_, _, k)| k == "unrelated").collect();
        let close_none = find_tools("close GUZ-22", 5).await.unwrap().none;
        for (task, _, _) in &unrelated {
            let n = find_tools(task, 5).await.unwrap().none;
            assert!(
                n > close_none,
                "task={} none={} close_none={}",
                task,
                n,
                close_none
            );
        }
    }

    #[tokio::test]
    async fn mcp_call_matches_shell_result() {
        let task = "close GUZ-22";
        let k = 5;
        let global = crate::Global { verbose: false };
        let mcp_result = super::super::handle_tools_call(
            Some(serde_json::json!({"name":"find_tools","arguments":{"task":task,"k":k}})),
            &global,
        )
        .await
        .unwrap();
        let shell_value = serde_json::to_value(find_tools(task, k).await.unwrap()).unwrap();
        assert_eq!(mcp_result["structuredContent"], shell_value);
    }

    struct Stub {
        status: u16,
        headers: Vec<(&'static str, &'static str)>,
        body: &'static str,
        delay_ms: u64,
    }

    impl Stub {
        fn ok(body: &'static str) -> Self {
            Self {
                status: 200,
                headers: vec![],
                body,
                delay_ms: 0,
            }
        }
    }

    async fn spawn_stub(stubs: Vec<Stub>, hits: Arc<AtomicUsize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => return,
                };
                let index = hits.fetch_add(1, Ordering::SeqCst);
                let stub = &stubs[index.min(stubs.len() - 1)];
                let mut raw = vec![0u8; 65536];
                let mut read = 0usize;
                while !raw[..read].windows(4).any(|w| w == b"\r\n\r\n") && read < raw.len() {
                    match stream.read(&mut raw[read..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => read += n,
                    }
                }
                let header_text = String::from_utf8_lossy(&raw[..read]).to_string();
                let content_len = header_text
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        match name.trim().eq_ignore_ascii_case("content-length") {
                            true => value.trim().parse::<usize>().ok(),
                            false => None,
                        }
                    })
                    .unwrap_or(0);
                let body_start = raw[..read]
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|i| i + 4)
                    .unwrap_or(read);
                let mut buffered = read.saturating_sub(body_start);
                while buffered < content_len {
                    match stream.read(&mut raw[..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buffered += n,
                    }
                }
                if stub.delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(stub.delay_ms)).await;
                }
                let mut extra = String::new();
                for (name, value) in &stub.headers {
                    extra.push_str(&format!("{name}: {value}\r\n"));
                }
                let response = format!(
                    "HTTP/1.1 {} x\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{}\r\n{}",
                    stub.status,
                    stub.body.len(),
                    extra,
                    stub.body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        url
    }

    #[tokio::test]
    async fn find_tools_with_uses_jev_on_split_probabilities() {
        let hits = Arc::new(AtomicUsize::new(0));
        let body = r#"{"answers":{"tool":{"probabilities":{"jira_update":0.71,"linear_issue_update":0.17,"none":0.10}}}}"#;
        let url = spawn_stub(vec![Stub::ok(body)], hits).await;
        let cfg = mcptools_core::jev::GatewayConfig {
            endpoint: url,
            model: "test".into(),
            api_key: mcptools_core::jev::Secret::new("k"),
        };
        let res = find_tools_with(Ok(Some(cfg)), "mark the ticket done", 5)
            .await
            .unwrap();
        assert_eq!(res.backend, Backend::Jev);
        let names: Vec<_> = res.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"jira_update"));
        assert!(names.contains(&"linear_issue_update"));
    }

    #[tokio::test]
    async fn find_tools_with_falls_back_to_local_on_529() {
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(
            vec![Stub {
                status: 529,
                headers: vec![],
                body: "",
                delay_ms: 0,
            }],
            hits,
        )
        .await;
        let cfg = mcptools_core::jev::GatewayConfig {
            endpoint: url,
            model: "test".into(),
            api_key: mcptools_core::jev::Secret::new("k"),
        };
        let jev_res = find_tools_with(Ok(Some(cfg)), "close GUZ-22", 5)
            .await
            .unwrap();
        assert_eq!(jev_res.backend, Backend::Local);
        let local_res = find_tools_with(Ok(None), "close GUZ-22", 5).await.unwrap();
        assert_eq!(jev_res.tools, local_res.tools);
    }
}
