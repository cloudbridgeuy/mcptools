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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListedTool {
    pub name: String,
    pub domain: String,
    pub declaration: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FindToolsOutput {
    Rank {
        none: f64,
        tools: Vec<FoundTool>,
        backend: Backend,
        #[serde(skip_serializing_if = "Option::is_none")]
        fallback: Option<FallbackReason>,
        usage: String,
    },
    Domain {
        domain: String,
        tools: Vec<ListedTool>,
        usage: String,
    },
    Domains {
        domains: Vec<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("set exactly one of task, domain, listDomains")]
    SetExactlyOne,
    #[error("domain is empty")]
    DomainEmpty,
    #[error("unknown domain '{0}'; valid domains: {1}")]
    UnknownDomain(String, String),
    #[error("k is only valid with task")]
    KOnlyForTask,
    #[error(transparent)]
    Rank(#[from] mcptools_core::find_tools::FindToolsError),
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindToolsArgs {
    pub task: Option<String>,
    pub k: Option<usize>,
    pub domain: Option<String>,
    #[serde(rename = "listDomains")]
    pub list_domains: Option<bool>,
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

fn list_domain_names() -> Vec<String> {
    let mut ds: Vec<String> = super::tool_catalog()
        .into_iter()
        .map(|e| e.domain)
        .collect();
    ds.sort();
    ds.dedup();
    ds
}

fn list_domain(domain: &str) -> Vec<ListedTool> {
    let reg = super::registered_tools();
    reg.iter()
        .filter(|t| t.name.split_once('_').map(|(p, _)| p).unwrap_or(&t.name) == domain)
        .map(|t| ListedTool {
            name: t.name.clone(),
            domain: domain.to_string(),
            declaration: super::declaration(t),
        })
        .collect()
}

fn usage_for(code_mode: bool) -> String {
    if code_mode {
        mcptools_core::find_tools::CODE_MODE_USAGE.to_string()
    } else {
        mcptools_core::find_tools::USAGE.to_string()
    }
}

pub async fn query(args: FindToolsArgs, code_mode: bool) -> Result<FindToolsOutput, QueryError> {
    let task = args.task.as_deref().map(str::trim);
    let domain = args.domain.as_deref().map(str::trim);
    let list = args.list_domains == Some(true);
    let task_p = task.is_some_and(|t| !t.is_empty());
    let domain_p = domain.is_some_and(|d| !d.is_empty());
    let list_p = list;
    let count = (task_p as u8) + (domain_p as u8) + (list_p as u8);
    if args.k.is_some() && !task_p {
        return Err(QueryError::KOnlyForTask);
    }
    if count != 1 {
        if domain.is_some_and(|d| d.is_empty()) && !task_p && !list_p {
            return Err(QueryError::DomainEmpty);
        }
        if task.is_some_and(|t| t.is_empty()) && !domain_p && !list_p {
            return Err(mcptools_core::find_tools::FindToolsError::EmptyTask.into());
        }
        return Err(QueryError::SetExactlyOne);
    }
    if list {
        let domains = list_domain_names();
        return Ok(FindToolsOutput::Domains { domains });
    }
    if let Some(d) = domain.filter(|&d| !d.is_empty()) {
        let d = d.to_string();
        let catalog = super::tool_catalog();
        let mut valids: Vec<String> = catalog.into_iter().map(|e| e.domain).collect();
        valids.sort();
        valids.dedup();
        if !valids.contains(&d) {
            return Err(QueryError::UnknownDomain(d, valids.join(", ")));
        }
        let tools = list_domain(&d);
        let usage = usage_for(code_mode);
        return Ok(FindToolsOutput::Domain {
            domain: d,
            tools,
            usage,
        });
    }
    let t = task.unwrap().to_string();
    let k = args.k.unwrap_or(mcptools_core::find_tools::DEFAULT_K);
    let inner = find_tools(&t, k).await?;
    let usage = usage_for(code_mode);
    Ok(FindToolsOutput::Rank {
        none: inner.none,
        tools: inner.tools,
        backend: inner.backend,
        fallback: inner.fallback,
        usage,
    })
}

pub async fn handle_find_tools(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
    flags: super::ServeFlags,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: FindToolsArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;
    let result = query(args, flags.code_mode)
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
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: false,
        };
        let mcp_result = super::super::handle_tools_call(
            Some(serde_json::json!({"name":"find_tools","arguments":{"task":task,"k":k}})),
            &global,
            flags,
        )
        .await
        .unwrap();
        let inner = find_tools(task, k).await.unwrap();
        let shell_value = serde_json::to_value(FindToolsOutput::Rank {
            none: inner.none,
            tools: inner.tools,
            backend: inner.backend,
            fallback: inner.fallback,
            usage: inner.usage,
        })
        .unwrap();
        assert_eq!(mcp_result["structuredContent"], shell_value);
    }

    fn test_flags(code_mode: bool) -> super::super::ServeFlags {
        super::super::ServeFlags {
            discovery: false,
            code_mode,
        }
    }

    fn test_global() -> crate::Global {
        crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        }
    }

    #[tokio::test]
    async fn usage_switches_by_code_mode() {
        let global = test_global();
        let args = Some(serde_json::json!({
            "name": "find_tools",
            "arguments": {"task": "search jira issues"}
        }));
        let plain = super::super::handle_tools_call(args.clone(), &global, test_flags(false))
            .await
            .unwrap();
        assert_eq!(
            plain["structuredContent"]["usage"],
            serde_json::json!(mcptools_core::find_tools::USAGE)
        );
        let code = super::super::handle_tools_call(args, &global, test_flags(true))
            .await
            .unwrap();
        assert_eq!(
            code["structuredContent"]["usage"],
            serde_json::json!(mcptools_core::find_tools::CODE_MODE_USAGE)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nested_find_tools_usage_follows_flags_inside_execute() {
        let global = test_global();
        let args = Some(serde_json::json!({
            "name": "execute",
            "arguments": {
                "code": "return (await find_tools({task: \"search jira issues\"})).usage"
            }
        }));
        let plain = super::super::handle_tools_call(args.clone(), &global, test_flags(false))
            .await
            .unwrap();
        assert_eq!(
            plain["structuredContent"]["result"],
            serde_json::json!(mcptools_core::find_tools::USAGE)
        );
        let code = super::super::handle_tools_call(args, &global, test_flags(true))
            .await
            .unwrap();
        assert_eq!(
            code["structuredContent"]["result"],
            serde_json::json!(mcptools_core::find_tools::CODE_MODE_USAGE)
        );
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
