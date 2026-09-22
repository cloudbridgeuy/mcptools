use super::JsonRpcError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FoundTool {
    pub name: String,
    pub domain: String,
    pub score: f64,
    pub declaration: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Jev,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    Unreachable,
    HttpStatus,
    InvalidResponse,
    InvalidConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FoundTools {
    pub none: f64,
    pub tools: Vec<FoundTool>,
    pub backend: Backend,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackReason>,
    pub usage: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindToolsArgs {
    #[schemars(description = "Natural-language description of the task")]
    pub task: String,
    #[schemars(description = "Maximum number of tools to return (default 5)")]
    pub k: Option<usize>,
}

fn attach_schemas(
    ranking: mcptools_core::find_tools::Ranking,
    backend: Backend,
    fallback: Option<FallbackReason>,
) -> FoundTools {
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
                    declaration: super::declaration(t),
                })
        })
        .collect();
    FoundTools {
        none: ranking.none,
        tools,
        backend,
        fallback,
        usage: mcptools_core::find_tools::USAGE.to_string(),
    }
}

pub async fn find_tools_with(
    config: Result<Option<mcptools_core::jev::GatewayConfig>, mcptools_core::jev::ConfigError>,
    task: &str,
    k: usize,
) -> Result<FoundTools, mcptools_core::find_tools::FindToolsError> {
    let catalog = super::tool_catalog();
    let local = mcptools_core::find_tools::rank_tools(task, &catalog, k)?;
    match config {
        Ok(Some(cfg)) => {
            let body = mcptools_core::jev::build_request(task, &catalog, &cfg.model);
            match crate::jev::classify(&cfg, &body).await {
                Ok(text) => match mcptools_core::jev::parse_ranking(&text, &catalog) {
                    Ok(r) => {
                        let sel = mcptools_core::jev::select(r, k);
                        Ok(attach_schemas(sel, Backend::Jev, None))
                    }
                    Err(mcptools_core::jev::ClassifyError::MalformedBody)
                    | Err(mcptools_core::jev::ClassifyError::MissingAnswer) => Ok(attach_schemas(
                        local,
                        Backend::Local,
                        Some(FallbackReason::InvalidResponse),
                    )),
                },
                Err(crate::jev::JevError::Unreachable) => Ok(attach_schemas(
                    local,
                    Backend::Local,
                    Some(FallbackReason::Unreachable),
                )),
                Err(crate::jev::JevError::HttpStatus) => Ok(attach_schemas(
                    local,
                    Backend::Local,
                    Some(FallbackReason::HttpStatus),
                )),
            }
        }
        Ok(None) => Ok(attach_schemas(local, Backend::Local, None)),
        Err(mcptools_core::jev::ConfigError::UnknownProvider)
        | Err(mcptools_core::jev::ConfigError::MissingKey) => Ok(attach_schemas(
            local,
            Backend::Local,
            Some(FallbackReason::InvalidConfig),
        )),
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
    async fn find_tools_attaches_declaration() {
        let result = find_tools("close GUZ-22", 5).await.unwrap();
        let registered = super::super::registered_tools();
        for found in &result.tools {
            let reg = registered.iter().find(|t| t.name == found.name).unwrap();
            assert_eq!(found.declaration, super::super::declaration(reg));
        }
        assert_eq!(result.usage, mcptools_core::find_tools::USAGE);
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
    async fn required_rows_each_pass() {
        let rows = parse_golden();
        let required: Vec<_> = rows.iter().filter(|(_, _, k)| k == "required").collect();
        let mut none_scores = vec![];
        let mut failing = vec![];
        for (task, expected, _) in &required {
            let res = find_tools(task, 5).await.unwrap();
            let names: Vec<_> = res.tools.iter().map(|t| t.name.as_str()).collect();
            if let Some(exps) = expected {
                let top1_ok = names.first().is_some_and(|t| exps.iter().any(|e| e == t));
                let all_in = exps.iter().all(|e| names.contains(&e.as_str()));
                none_scores.push(res.none);
                if !(top1_ok && all_in) {
                    failing.push(task.clone());
                }
            }
        }
        for (task, expected, _) in &required {
            if expected.is_none() {
                let res = find_tools(task, 5).await.unwrap();
                if !none_scores.iter().all(|&n| n < res.none) {
                    failing.push(task.clone());
                }
            }
        }
        assert!(failing.is_empty(), "required rows failed: {failing:?}");
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
        let global = crate::Global {
            verbose: false,
            discovery: false,
        };
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
        assert_eq!(jev_res.fallback, Some(FallbackReason::HttpStatus));
        let local_res = find_tools_with(Ok(None), "close GUZ-22", 5).await.unwrap();
        assert_eq!(jev_res.tools, local_res.tools);
        assert_eq!(local_res.fallback, None);
    }

    #[tokio::test]
    async fn fallback_on_garbage_body_is_invalid_response() {
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(vec![Stub::ok("not json")], hits).await;
        let cfg = mcptools_core::jev::GatewayConfig {
            endpoint: url,
            model: "test".into(),
            api_key: mcptools_core::jev::Secret::new("k"),
        };
        let res = find_tools_with(Ok(Some(cfg)), "close GUZ-22", 5)
            .await
            .unwrap();
        assert_eq!(res.backend, Backend::Local);
        assert_eq!(res.fallback, Some(FallbackReason::InvalidResponse));
    }

    #[tokio::test]
    async fn fallback_on_closed_port_is_unreachable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{}", addr);
        let cfg = mcptools_core::jev::GatewayConfig {
            endpoint: url,
            model: "test".into(),
            api_key: mcptools_core::jev::Secret::new("k"),
        };
        let res = find_tools_with(Ok(Some(cfg)), "close GUZ-22", 5)
            .await
            .unwrap();
        assert_eq!(res.backend, Backend::Local);
        assert_eq!(res.fallback, Some(FallbackReason::Unreachable));
    }

    #[tokio::test]
    async fn fallback_on_401_is_http_status() {
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(
            vec![Stub {
                status: 401,
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
        let res = find_tools_with(Ok(Some(cfg)), "close GUZ-22", 5)
            .await
            .unwrap();
        assert_eq!(res.backend, Backend::Local);
        assert_eq!(res.fallback, Some(FallbackReason::HttpStatus));
    }

    #[tokio::test]
    async fn fallback_on_unknown_provider_is_invalid_config() {
        let err = mcptools_core::jev::gateway_config(|k| {
            (k == "JEV_PROVIDER").then(|| "bogus".to_string())
        });
        let res = find_tools_with(err, "close GUZ-22", 5).await.unwrap();
        assert_eq!(res.backend, Backend::Local);
        assert_eq!(res.fallback, Some(FallbackReason::InvalidConfig));
    }

    #[tokio::test]
    async fn fallback_on_missing_key_is_invalid_config() {
        let err = mcptools_core::jev::gateway_config(|k| {
            (k == "JEV_PROVIDER").then(|| "opencode".to_string())
        });
        let res = find_tools_with(err, "close GUZ-22", 5).await.unwrap();
        assert_eq!(res.backend, Backend::Local);
        assert_eq!(res.fallback, Some(FallbackReason::InvalidConfig));
    }

    #[tokio::test]
    async fn secret_never_leaks() {
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(
            vec![Stub {
                status: 401,
                headers: vec![],
                body: "",
                delay_ms: 0,
            }],
            hits,
        )
        .await;
        let sentinel = "sentinel-key-zz99";
        let cfg = mcptools_core::jev::GatewayConfig {
            endpoint: url,
            model: "test".into(),
            api_key: mcptools_core::jev::Secret::new(sentinel),
        };
        assert!(!format!("{:?}", cfg).contains(sentinel));
        let res = find_tools_with(Ok(Some(cfg)), "close GUZ-22", 5)
            .await
            .unwrap();
        let json = serde_json::to_string(&res).unwrap();
        assert!(!json.contains(sentinel));
        assert!(!format!("{:?}", mcptools_core::jev::ConfigError::MissingKey).contains(sentinel));
        assert!(!format!("{:?}", crate::jev::JevError::HttpStatus).contains(sentinel));
        assert!(
            !format!("{:?}", mcptools_core::jev::ClassifyError::MalformedBody).contains(sentinel)
        );
    }
}
