use crate::linear::config::LinearConfig;
use crate::prelude::*;
use mcptools_core::linear::{
    backoff_ms, check_response, classify_retry, first_error_code, is_mutation, MAX_READ_ATTEMPTS,
};

pub const LINEAR_API_URL: &str = "https://api.linear.app/graphql";
const JITTER_MS: u64 = 100;

pub fn build_client(cfg: &LinearConfig) -> Result<reqwest::Client> {
    build_client_with_timeout(cfg, std::time::Duration::from_secs(10))
}

pub fn build_client_with_timeout(
    cfg: &LinearConfig,
    timeout: std::time::Duration,
) -> Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&cfg.api_key).map_err(|e| eyre!("Invalid Linear API key: {}", e))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(timeout)
        .build()
        .map_err(|e| eyre!("Failed to build HTTP client: {}", e))
}

pub async fn execute(
    client: &reqwest::Client,
    query: &str,
    variables: serde_json::Value,
) -> Result<serde_json::Value> {
    execute_with_url(client, LINEAR_API_URL, query, variables).await
}

async fn execute_with_url(
    client: &reqwest::Client,
    url: &str,
    query: &str,
    variables: serde_json::Value,
) -> Result<serde_json::Value> {
    if !variables.is_object() {
        return Err(eyre!("GraphQL variables must be a JSON object"));
    }
    let readonly = !is_mutation(query);
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        let response = match client
            .post(url)
            .json(&serde_json::json!({"query": query, "variables": variables}))
            .send()
            .await
        {
            Err(e) if e.is_timeout() => return uncertain_outcome(e),
            Err(e) => return Err(eyre!("Failed to send request to Linear: {}", e)),
            Ok(response) => response,
        };
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = match response.text().await {
            Err(e) if e.is_timeout() => return uncertain_outcome(e),
            Err(e) => return Err(eyre!("Failed to read Linear response body: {}", e)),
            Ok(body) => body,
        };
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .map(|parsed| first_error_code(&parsed))
            .unwrap_or_default();
        if let Some(hint) = classify_retry(status, &code, &headers) {
            if !readonly {
                let shown = match code.is_empty() {
                    true => "absent",
                    false => code.as_str(),
                };
                return Err(eyre!(
                    "Linear mutation was not automatically retried because the outcome may be uncertain (status {}, error code {}). Re-query by ID before deciding to resend; retry after {}ms",
                    status,
                    shown,
                    hint
                ));
            }
            if attempt >= MAX_READ_ATTEMPTS {
                return match check_response(status, &body) {
                    Ok(data) => Ok(data),
                    Err(e) => Err(eyre!(
                        "Linear request still rate-limited after {} attempts: {}",
                        MAX_READ_ATTEMPTS,
                        e
                    )),
                };
            }
            let wait = backoff_ms(attempt, hint);
            let jitter = {
                use rand::Rng;
                rand::thread_rng().gen_range(0..=JITTER_MS)
            };
            tokio::time::sleep(std::time::Duration::from_millis(
                wait.saturating_add(jitter),
            ))
            .await;
            continue;
        }
        return check_response(status, &body).map_err(|e| eyre!("{}", e));
    }
}

fn uncertain_outcome<T>(e: reqwest::Error) -> Result<T> {
    Err(eyre!(
        "Linear request timed out; outcome is uncertain and the operation was not retried. Re-query by ID before deciding to resend: {}",
        e
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        let url = format!("http://{}/graphql", listener.local_addr().unwrap());
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

    async fn run_query(
        query: &str,
        stubs: Vec<Stub>,
        hits: Arc<AtomicUsize>,
    ) -> Result<serde_json::Value> {
        let served = spawn_stub(stubs, Arc::clone(&hits)).await;
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let _ = build_client(&cfg).unwrap();
        execute_with_url(&client, &served, query, serde_json::json!({})).await
    }

    #[test]
    fn missing_key_error_names_variable() {
        let saved = std::env::var("LINEAR_API_KEY").ok();
        std::env::remove_var("LINEAR_API_KEY");
        let err = LinearConfig::from_env().unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains("LINEAR_API_KEY"));
        if let Some(key) = saved {
            std::env::set_var("LINEAR_API_KEY", key);
        }
    }

    #[tokio::test]
    async fn redacts_key_in_debug_and_error_output() {
        let fixture_key = "lin_api_3x4mpl3_s3cr3t_k3y_zz99";
        let cfg = LinearConfig {
            api_key: fixture_key.to_string(),
        };
        assert!(!format!("{cfg:?}").contains(fixture_key));
        let client = build_client(&cfg).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(
            vec![Stub {
                status: 500,
                headers: vec![],
                body: r#"{"errors":[{"message":"boom"}]}"#,
                delay_ms: 0,
            }],
            Arc::clone(&hits),
        )
        .await;
        let err = execute_with_url(
            &client,
            &url,
            "query { viewer { id } }",
            serde_json::json!({}),
        )
        .await
        .unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains("500"));
        assert!(!text.contains(fixture_key));
    }

    #[tokio::test]
    async fn rejects_non_object_variables_without_request() {
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(vec![Stub::ok(r#"{"data":{}}"#)], Arc::clone(&hits)).await;
        let client = reqwest::Client::builder().build().unwrap();
        let err = execute_with_url(&client, &url, "query { x }", serde_json::json!([]))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must be a JSON object"));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reports_timeout_as_uncertain_without_retry() {
        let cfg = LinearConfig {
            api_key: "timeout-key".to_string(),
        };
        let client =
            build_client_with_timeout(&cfg, std::time::Duration::from_millis(100)).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let url = spawn_stub(
            vec![Stub {
                status: 200,
                headers: vec![],
                body: r#"{"data":{"viewer":{"id":"u1"}}}"#,
                delay_ms: 2000,
            }],
            Arc::clone(&hits),
        )
        .await;
        let err = execute_with_url(
            &client,
            &url,
            "query { viewer { id } }",
            serde_json::json!({}),
        )
        .await
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("uncertain"), "{text}");
        assert!(text.contains("query by ID"), "{text}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_malformed_json_body() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![Stub::ok("not json")],
            Arc::clone(&hits),
        )
        .await;
        let err = out.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("parse"), "{err:?}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_graphql_errors_without_retry() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![Stub::ok(
                r#"{"data":null,"errors":[{"message":"Cannot query field"}]}"#,
            )],
            Arc::clone(&hits),
        )
        .await;
        let err = out.unwrap_err();
        assert!(err.to_string().contains("Cannot query field"), "{err:?}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_partial_data_with_both_sides() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![Stub::ok(
                r#"{"data":{"viewer":{"id":"u9"}},"errors":[{"message":"Field timed out"}]}"#,
            )],
            Arc::clone(&hits),
        )
        .await;
        let err = out.unwrap_err();
        let text = err.to_string();
        assert!(text.contains("Field timed out"), "{text}");
        assert!(text.contains("u9"), "{text}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_rate_limited_query_then_succeeds() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![
                Stub {
                    status: 429,
                    headers: vec![("retry-after", "0")],
                    body: r#"{"errors":[{"message":"Slow down"}]}"#,
                    delay_ms: 0,
                },
                Stub::ok(r#"{"data":{"viewer":{"id":"u1","name":"Ada"}}}"#),
            ],
            Arc::clone(&hits),
        )
        .await;
        let data = out.unwrap();
        assert_eq!(
            data.get("viewer")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str()),
            Some("u1")
        );
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retries_ratelimited_code_then_succeeds() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![
                Stub {
                    status: 200,
                    headers: vec![("retry-after", "0")],
                    body: r#"{"data":null,"errors":[{"message":"Slow down","extensions":{"code":"RATELIMITED"}}]}"#,
                    delay_ms: 0,
                },
                Stub::ok(r#"{"data":{"viewer":{"id":"u2","name":"Bo"}}}"#),
            ],
            Arc::clone(&hits),
        )
        .await;
        let data = out.unwrap();
        assert_eq!(
            data.get("viewer")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str()),
            Some("u2")
        );
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stops_after_three_rate_limited_attempts() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "query { viewer { id } }",
            vec![Stub {
                status: 429,
                headers: vec![("retry-after", "0")],
                body: r#"{"errors":[{"message":"Slow down"}]}"#,
                delay_ms: 0,
            }],
            Arc::clone(&hits),
        )
        .await;
        let err = out.unwrap_err();
        let text = err.to_string();
        assert!(text.contains("3 attempts"), "{text}");
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn never_retries_mutation() {
        let hits = Arc::new(AtomicUsize::new(0));
        let out = run_query(
            "mutation { createIssue(input: {}) { id } }",
            vec![Stub {
                status: 429,
                headers: vec![("retry-after", "0")],
                body: r#"{"errors":[{"message":"Slow down"}]}"#,
                delay_ms: 0,
            }],
            Arc::clone(&hits),
        )
        .await;
        let err = out.unwrap_err();
        let text = err.to_string();
        assert!(text.contains("not automatically retried"), "{text}");
        assert!(text.contains("query by ID"), "{text}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
