use super::{call_tool, execute, ServeFlags};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn global() -> crate::Global {
    crate::Global {
        verbose: false,
        execute_timeout_secs: 30,
        execute_memory_mb: 64,
        execute_output_kb: 256,
    }
}

fn flags() -> ServeFlags {
    ServeFlags {
        discovery: false,
        code_mode: true,
    }
}

async fn call(input: Value) -> Result<Value, super::JsonRpcError> {
    call_tool::handle_call_tool(
        Some(json!({"name":"http_request", "input":input, "allowWrites":true})),
        &global(),
        flags(),
    )
    .await
}

async fn server(response: Vec<u8>, delay: u64) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        let mut buf = [0; 8192];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buf[..n]);
            if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&received[..end]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                if received.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        let _ = stream.write_all(&response).await;
        received
    });
    (url, task)
}

#[tokio::test]
async fn allowlist_denies_empty_and_other_hosts() {
    let (url, task) = server(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(), 0).await;
    for allowed in ["", "example.org"] {
        std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", allowed);
        let error = call(json!({"method":"GET", "url":url})).await.unwrap_err();
        assert_eq!(
            error.message,
            "HTTP host is not allowed by MCPTOOLS_HTTP_ALLOWED_HOSTS"
        );
    }
    task.abort();
}

#[tokio::test]
async fn streams_file_with_exact_length_and_execute_chain() {
    std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", "127.0.0.1");
    let file = tempfile::NamedTempFile::new().unwrap();
    let bytes = [0, 255, 128, 1, 2, 13, 10];
    std::fs::write(file.path(), bytes).unwrap();
    let (url, task) = server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
        0,
    )
    .await;
    let code = format!("const s = await file_stat({{path:{}}}); await http_request({{method:'POST',url:{},headers:{{'x-size':String(s.size)}},bodyPath:{}}})", json!(file.path().to_str().unwrap()), json!(url), json!(file.path().to_str().unwrap()));
    let result = execute::handle_execute(
        Some(json!({"code":code, "allowWrites":true})),
        &global(),
        flags(),
    )
    .await
    .unwrap();
    assert_eq!(result["structuredContent"]["result"]["status"], 200);
    let request = task.await.unwrap();
    let end = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
    assert!(headers.contains("content-length: 7"));
    assert!(headers.contains("x-size: 7"));
    assert_eq!(&request[end + 4..], bytes);
}

#[tokio::test]
async fn foreign_redirect_is_not_followed() {
    std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", "127.0.0.1");
    let (url, task) = server(
        b"HTTP/1.1 302 Found\r\nLocation: http://localhost:1/\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
        0,
    )
    .await;
    let result = call(json!({"method":"GET", "url":url})).await.unwrap();
    assert_eq!(result["structuredContent"]["status"], 302);
    task.await.unwrap();
}

#[tokio::test]
async fn response_is_capped() {
    std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", "127.0.0.1");
    let mut response = b"HTTP/1.1 200 OK\r\nContent-Length: 2097152\r\n\r\n".to_vec();
    response.extend(vec![b'x'; 2097152]);
    let (url, task) = server(response, 0).await;
    let error = call(json!({"method":"GET", "url":url})).await.unwrap_err();
    assert_eq!(error.message, "HTTP response exceeds 1 MiB");
    task.await.unwrap();
}

#[tokio::test]
async fn response_times_out() {
    std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", "127.0.0.1");
    let (url, task) = server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
        11000,
    )
    .await;
    let started = std::time::Instant::now();
    let error = call(json!({"method":"GET", "url":url})).await.unwrap_err();
    assert_eq!(error.message, "HTTP request failed or timed out");
    assert!(started.elapsed() >= std::time::Duration::from_secs(9));
    task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn fifo_body_is_rejected_without_waiting_for_a_writer() {
    std::env::set_var("MCPTOOLS_HTTP_ALLOWED_HOSTS", "127.0.0.1");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("body");
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let mut request = tokio::spawn(super::http_request::request(Some(
        json!({"method":"POST", "url":"http://127.0.0.1:9", "bodyPath":path}),
    )));
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), &mut request).await;
    let result = match result {
        Ok(result) => result.unwrap(),
        Err(_) => {
            let _release = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            let _ = request.await.unwrap();
            panic!("FIFO body waited for a writer");
        }
    };
    assert!(result.unwrap_err().message.contains("not a regular file"));
}
