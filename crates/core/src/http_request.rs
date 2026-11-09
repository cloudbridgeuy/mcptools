use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RequestArgs {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub body_path: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatArgs {
    pub path: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct StatOutput {
    pub size: u64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::none()
}

pub fn validate(
    args: &RequestArgs,
    allowed: &str,
) -> Result<(reqwest::Method, reqwest::Url), &'static str> {
    let method = args
        .method
        .parse::<reqwest::Method>()
        .map_err(|_| "Invalid HTTP method")?;
    if !matches!(
        method,
        reqwest::Method::GET
            | reqwest::Method::POST
            | reqwest::Method::PUT
            | reqwest::Method::PATCH
            | reqwest::Method::DELETE
            | reqwest::Method::HEAD
            | reqwest::Method::OPTIONS
    ) {
        return Err("Unsupported HTTP method");
    }
    let url = reqwest::Url::parse(&args.url).map_err(|_| "Invalid HTTP URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Invalid HTTP URL");
    }
    let host = url.host_str().ok_or("Invalid HTTP URL")?;
    if !allowed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|s| s.eq_ignore_ascii_case(host))
    {
        return Err("HTTP host is not allowed by [http].allowed_hosts or MCPTOOLS_HTTP_ALLOWED_HOSTS override");
    }
    if args.body.is_some() && args.body_path.is_some() {
        return Err("body and bodyPath are mutually exclusive");
    }
    for (name, value) in &args.headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| "Invalid request header")?;
        if name == reqwest::header::HOST
            || name == reqwest::header::CONTENT_LENGTH
            || name == reqwest::header::TRANSFER_ENCODING
        {
            return Err("Request header is managed by mcptools");
        }
        reqwest::header::HeaderValue::from_str(value).map_err(|_| "Invalid request header")?;
    }
    Ok((method, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(url: &str) -> RequestArgs {
        RequestArgs {
            method: "GET".into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            body_path: None,
        }
    }

    #[test]
    fn hosts_are_exact_and_urls_cannot_add_credentials() {
        let request = args("https://example.com/path");
        assert!(validate(&request, " EXAMPLE.COM, other.example ").is_ok());
        for allowed in [
            "",
            "*.com",
            "example.com.evil",
            "https://example.com",
            "example.com:443",
        ] {
            assert!(validate(&request, allowed).is_err());
        }
        for url in [
            "https://sub.example.com",
            "https://user:secret@example.com",
            "file:///example.com",
            "invalid",
        ] {
            assert!(validate(&args(url), "example.com").is_err());
        }
    }

    #[test]
    fn framing_headers_and_conflicting_bodies_are_rejected_without_values() {
        let mut request = args("https://example.com");
        for name in ["Host", "content-length", "TRANSFER-ENCODING"] {
            request.headers.clear();
            request.headers.insert(name.into(), "secret".into());
            assert_eq!(
                validate(&request, "example.com").unwrap_err(),
                "Request header is managed by mcptools"
            );
        }
        request.headers.clear();
        request
            .headers
            .insert("authorization".into(), "secret\r\nvalue".into());
        assert_eq!(
            validate(&request, "example.com").unwrap_err(),
            "Invalid request header"
        );
        request.headers.clear();
        request.body = Some("body".into());
        request.body_path = Some("path".into());
        assert_eq!(
            validate(&request, "example.com").unwrap_err(),
            "body and bodyPath are mutually exclusive"
        );
        request.body_path = None;
        request.method = "CONNECT".into();
        assert_eq!(
            validate(&request, "example.com").unwrap_err(),
            "Unsupported HTTP method"
        );
    }
}
