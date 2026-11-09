use super::JsonRpcError;
use mcptools_core::http_request::{self, RequestArgs, Response, StatArgs, StatOutput};
use serde_json::Value;
use tokio::io::AsyncReadExt;

fn failure(message: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: message.into(),
        data: None,
    }
}

pub async fn stat(arguments: Option<Value>) -> Result<Value, JsonRpcError> {
    let args: StatArgs = serde_json::from_value(arguments.unwrap_or(Value::Null))
        .map_err(|_| failure("Invalid file_stat arguments"))?;
    let metadata = tokio::fs::metadata(args.path)
        .await
        .map_err(|_| failure("Cannot stat file"))?;
    if !metadata.is_file() {
        return Err(failure("Path is not a regular file"));
    }
    super::to_dual_result(StatOutput {
        size: metadata.len(),
    })
}

pub async fn request(arguments: Option<Value>) -> Result<Value, JsonRpcError> {
    let allowed = std::env::var("MCPTOOLS_HTTP_ALLOWED_HOSTS").unwrap_or_default();
    request_with_allowlist(arguments, &allowed).await
}

pub async fn request_with_allowlist(
    arguments: Option<Value>,
    allowed: &str,
) -> Result<Value, JsonRpcError> {
    let args: RequestArgs = serde_json::from_value(arguments.unwrap_or(Value::Null))
        .map_err(|_| failure("Invalid http_request arguments"))?;
    let (method, url) = http_request::validate(&args, allowed).map_err(failure)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(http_request::redirect_policy())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| failure("Cannot create HTTP client"))?;
    let mut request = client.request(method, url);
    for (name, value) in args.headers {
        request = request.header(name, value);
    }
    if let Some(path) = args.body_path {
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| failure("Cannot stat request body file"))?;
        if !metadata.is_file() {
            return Err(failure("Request body path is not a regular file"));
        }
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| failure("Cannot open request body file"))?;
        let metadata = file
            .metadata()
            .await
            .map_err(|_| failure("Cannot stat request body file"))?;
        if !metadata.is_file() {
            return Err(failure("Request body path is not a regular file"));
        }
        let stream = futures::stream::try_unfold(file, |mut file| async move {
            let mut buffer = vec![0; 65536];
            let size = file.read(&mut buffer).await?;
            buffer.truncate(size);
            Ok::<_, std::io::Error>((size != 0).then_some((buffer, file)))
        });
        request = request
            .header(reqwest::header::CONTENT_LENGTH, metadata.len())
            .body(reqwest::Body::wrap_stream(stream));
    } else if let Some(body) = args.body {
        request = request.body(body);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| failure("HTTP request failed or timed out"))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| failure("HTTP response failed or timed out"))?
    {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(failure("HTTP response exceeds 1 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(bytes).map_err(|_| failure("HTTP response is not UTF-8"))?;
    super::to_dual_result(Response {
        status,
        headers,
        body,
    })
}
