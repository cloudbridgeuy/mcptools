//! Transformation functions for Confluence API responses

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Confluence page response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfluencePageResponse {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "_links")]
    pub links: PageLinks,
    #[serde(default)]
    pub body: Option<PageBody>,
}

/// Links from page response
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PageLinks {
    #[serde(default)]
    pub webui: Option<String>,
}

/// Body content from page
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PageBody {
    #[serde(default)]
    pub view: Option<ViewContent>,
}

/// View content (HTML)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ViewContent {
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfluenceSearchResponse {
    results: Vec<ConfluenceSearchResult>,
    #[serde(default)]
    size: usize,
    #[serde(default, rename = "totalSize")]
    total_size: usize,
}

#[derive(Debug, Deserialize)]
struct ConfluenceSearchResult {
    content: ConfluencePageResponse,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, JsonSchema)]
pub struct PageOutput {
    pub id: String,
    pub title: String,
    pub page_type: String,
    pub url: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SearchOutput {
    pub pages: Vec<PageOutput>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchRequest {
    pub url: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum SearchError {
    #[error("invalid Confluence base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("invalid Confluence search response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetPageRequest {
    pub page_id: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GetPageOutput {
    pub id: String,
    pub title: String,
    pub space_id: String,
    pub version: i32,
    pub body: String,
    pub url: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum GetPageError {
    #[error("Confluence page must be a numeric ID or a URL containing a page ID")]
    InvalidPageReference,
    #[error("invalid Confluence base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("invalid Confluence page response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WritePageRequest {
    pub url: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdatePageSource {
    pub id: String,
    pub title: String,
    pub space_id: String,
    pub version: i32,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdatePageChanges {
    title: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct WritePageOutput {
    pub id: String,
    pub title: String,
    pub space_id: String,
    pub version: i32,
    pub url: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum WritePageError {
    #[error("Confluence page must be a numeric ID or a URL containing a page ID")]
    InvalidPageReference,
    #[error("{0} must not be empty")]
    EmptyField(&'static str),
    #[error("title or body is required")]
    NoChanges,
    #[error("page response ID does not match requested page")]
    PageIdMismatch,
    #[error("page version cannot be incremented")]
    VersionOverflow,
    #[error("invalid Confluence base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("invalid Confluence page response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Deserialize)]
struct UpdatePageResponse {
    id: String,
    title: String,
    #[serde(rename = "spaceId")]
    space_id: String,
    version: GetPageVersion,
    body: UpdatePageBody,
}

#[derive(Debug, Deserialize)]
struct UpdatePageBody {
    storage: Option<GetPageBodyValue>,
}

#[derive(Debug, Deserialize)]
struct WritePageResponse {
    id: String,
    title: String,
    #[serde(rename = "spaceId")]
    space_id: String,
    version: GetPageVersion,
    #[serde(default, rename = "_links")]
    links: Option<GetPageLinks>,
}

pub fn update_page_changes(
    title: Option<String>,
    body: Option<String>,
) -> Result<UpdatePageChanges, WritePageError> {
    if title.as_ref().is_some_and(|value| value.trim().is_empty()) {
        return Err(WritePageError::EmptyField("title"));
    }
    if title.is_none() && body.is_none() {
        return Err(WritePageError::NoChanges);
    }
    Ok(UpdatePageChanges { title, body })
}

pub fn build_create_page_request(
    base_url: &str,
    space_id: &str,
    title: &str,
    body: &str,
    parent: Option<&str>,
) -> Result<WritePageRequest, WritePageError> {
    if space_id.trim().is_empty() {
        return Err(WritePageError::EmptyField("space ID"));
    }
    if title.trim().is_empty() {
        return Err(WritePageError::EmptyField("title"));
    }
    let url = confluence_api_url(base_url, "pages")?;
    let mut payload = serde_json::json!({
        "spaceId": space_id,
        "status": "current",
        "title": title,
        "body": {"representation": "storage", "value": body}
    });
    if let Some(parent) = parent {
        let parent_id =
            page_id_from_reference(parent).map_err(|_| WritePageError::InvalidPageReference)?;
        payload["parentId"] = serde_json::Value::String(parent_id);
    }
    Ok(WritePageRequest { url, body: payload })
}

pub fn build_update_page_source_request(
    base_url: &str,
    id_or_url: &str,
) -> Result<GetPageRequest, WritePageError> {
    let page_id =
        page_id_from_reference(id_or_url).map_err(|_| WritePageError::InvalidPageReference)?;
    let url = format!(
        "{}?body-format=storage",
        confluence_api_url(base_url, &format!("pages/{page_id}"))?
    );
    Ok(GetPageRequest { page_id, url })
}

pub fn parse_update_page_source(response: &str) -> Result<UpdatePageSource, WritePageError> {
    let page: UpdatePageResponse = serde_json::from_str(response)
        .map_err(|error| WritePageError::InvalidResponse(error.to_string()))?;
    let body = page
        .body
        .storage
        .ok_or_else(|| WritePageError::InvalidResponse("missing storage body".to_string()))?;
    Ok(UpdatePageSource {
        id: page.id,
        title: page.title,
        space_id: page.space_id,
        version: page.version.number,
        body: body.value,
    })
}

pub fn build_update_page_request(
    base_url: &str,
    requested_page_id: &str,
    source: &UpdatePageSource,
    changes: &UpdatePageChanges,
) -> Result<WritePageRequest, WritePageError> {
    if source.id != requested_page_id {
        return Err(WritePageError::PageIdMismatch);
    }
    let version = source
        .version
        .checked_add(1)
        .ok_or(WritePageError::VersionOverflow)?;
    let title = changes.title.as_deref().unwrap_or(&source.title);
    let body = changes.body.as_deref().unwrap_or(&source.body);
    let url = confluence_api_url(base_url, &format!("pages/{}", source.id))?;
    Ok(WritePageRequest {
        url,
        body: serde_json::json!({
            "id": source.id,
            "status": "current",
            "title": title,
            "body": {"representation": "storage", "value": body},
            "version": {"number": version}
        }),
    })
}

pub fn parse_write_page_response(
    response: &str,
    base_url: &str,
) -> Result<WritePageOutput, WritePageError> {
    let page: WritePageResponse = serde_json::from_str(response)
        .map_err(|error| WritePageError::InvalidResponse(error.to_string()))?;
    let links = page.links.as_ref();
    let wiki_base = wiki_base_url(base_url, links.and_then(|value| value.base.as_deref()))
        .map_err(|error| WritePageError::InvalidBaseUrl(error.to_string()))?;
    let url = page_url(
        &wiki_base,
        links.and_then(|value| value.webui.as_deref()),
        &page.id,
    )
    .map_err(|error| WritePageError::InvalidResponse(error.to_string()))?;
    Ok(WritePageOutput {
        id: page.id,
        title: page.title,
        space_id: page.space_id,
        version: page.version.number,
        url,
    })
}

fn confluence_api_url(base_url: &str, resource: &str) -> Result<String, WritePageError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| WritePageError::InvalidBaseUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(WritePageError::InvalidBaseUrl(base_url.to_string()));
    }
    let base_path = url.path().trim_end_matches('/');
    let path = if base_path.ends_with("/wiki") {
        format!("{base_path}/api/v2/{resource}")
    } else {
        format!("{base_path}/wiki/api/v2/{resource}")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.into())
}

pub fn build_search_request(
    base_url: &str,
    query: &str,
    limit: usize,
) -> Result<SearchRequest, SearchError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| SearchError::InvalidBaseUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SearchError::InvalidBaseUrl(base_url.to_string()));
    }
    let base_path = url.path().trim_end_matches('/');
    let path = if base_path.ends_with("/wiki") {
        format!("{base_path}/rest/api/search")
    } else {
        format!("{base_path}/wiki/rest/api/search")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut()
        .append_pair("cql", query)
        .append_pair("limit", &limit.to_string())
        .append_pair("expand", "content.body.view");
    Ok(SearchRequest { url: url.into() })
}

pub fn parse_search_response(response: &str) -> Result<SearchOutput, SearchError> {
    let response: ConfluenceSearchResponse = serde_json::from_str(response)
        .map_err(|error| SearchError::InvalidResponse(error.to_string()))?;
    let total = response.total_size.max(response.size);
    let pages = response
        .results
        .into_iter()
        .map(|result| {
            let page = result.content;
            let content = page
                .body
                .as_ref()
                .and_then(|body| body.view.as_ref())
                .and_then(|view| view.value.as_ref())
                .map(|html| html_to_plaintext(html));
            PageOutput {
                id: page.id,
                title: page.title,
                page_type: page.page_type,
                url: result.url.or(page.links.webui),
                content,
            }
        })
        .collect();
    Ok(SearchOutput { pages, total })
}

#[derive(Debug, Deserialize)]
struct GetPageResponse {
    id: String,
    title: String,
    #[serde(rename = "spaceId")]
    space_id: String,
    version: GetPageVersion,
    body: GetPageBody,
    #[serde(rename = "_links")]
    links: GetPageLinks,
}

#[derive(Debug, Deserialize)]
struct GetPageVersion {
    number: i32,
}

#[derive(Debug, Deserialize)]
struct GetPageBody {
    view: Option<GetPageBodyValue>,
}

#[derive(Debug, Deserialize)]
struct GetPageBodyValue {
    value: String,
}

#[derive(Debug, Deserialize)]
struct GetPageLinks {
    webui: Option<String>,
    base: Option<String>,
}

pub fn build_get_page_request(
    base_url: &str,
    id_or_url: &str,
) -> Result<GetPageRequest, GetPageError> {
    let page_id = page_id_from_reference(id_or_url)?;
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| GetPageError::InvalidBaseUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(GetPageError::InvalidBaseUrl(base_url.to_string()));
    }
    let base_path = url.path().trim_end_matches('/');
    let path = if base_path.ends_with("/wiki") {
        format!("{base_path}/api/v2/pages/{page_id}")
    } else {
        format!("{base_path}/wiki/api/v2/pages/{page_id}")
    };
    url.set_path(&path);
    url.set_query(Some("body-format=view"));
    url.set_fragment(None);
    Ok(GetPageRequest {
        page_id,
        url: url.into(),
    })
}

pub fn parse_get_page_response(
    response: &str,
    base_url: &str,
) -> Result<GetPageOutput, GetPageError> {
    let page: GetPageResponse = serde_json::from_str(response)
        .map_err(|error| GetPageError::InvalidResponse(error.to_string()))?;
    let body = page
        .body
        .view
        .ok_or_else(|| GetPageError::InvalidResponse("missing view body".to_string()))?;
    let wiki_base = wiki_base_url(base_url, page.links.base.as_deref())?;
    let url = page_url(&wiki_base, page.links.webui.as_deref(), &page.id)?;
    let html = absolutize_links(&body.value, &wiki_base);
    Ok(GetPageOutput {
        id: page.id,
        title: page.title,
        space_id: page.space_id,
        version: page.version.number,
        body: html2md::parse_html(&html).trim().to_string(),
        url,
    })
}

pub fn page_id_from_reference(id_or_url: &str) -> Result<String, GetPageError> {
    let reference = id_or_url.trim();
    if is_page_id(reference) {
        return Ok(reference.to_string());
    }
    let url = reqwest::Url::parse(reference).map_err(|_| GetPageError::InvalidPageReference)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(GetPageError::InvalidPageReference);
    }
    if let Some(page_id) = url.query_pairs().find_map(|(key, value)| {
        (key == "pageId" && is_page_id(&value)).then(|| value.into_owned())
    }) {
        return Ok(page_id);
    }
    let segments = url
        .path_segments()
        .ok_or(GetPageError::InvalidPageReference)?
        .collect::<Vec<_>>();
    segments
        .windows(2)
        .find_map(|parts| {
            (parts[0] == "pages" && is_page_id(parts[1])).then(|| parts[1].to_string())
        })
        .ok_or(GetPageError::InvalidPageReference)
}

fn is_page_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// Resolve the Confluence wiki base URL (for example `https://site/wiki`).
///
/// Prefers the API's `_links.base`; otherwise derives it from the configured
/// site URL, appending `/wiki` when missing.
fn wiki_base_url(base_url: &str, links_base: Option<&str>) -> Result<reqwest::Url, GetPageError> {
    if let Some(url) = links_base
        .and_then(|value| reqwest::Url::parse(value).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
    {
        return Ok(with_trailing_slash(url));
    }
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| GetPageError::InvalidBaseUrl(error.to_string()))?;
    let base_path = url.path().trim_end_matches('/').to_string();
    if !base_path.ends_with("/wiki") {
        url.set_path(&format!("{base_path}/wiki"));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(with_trailing_slash(url))
}

fn with_trailing_slash(mut url: reqwest::Url) -> reqwest::Url {
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url
}

/// Resolve a Confluence link against the wiki base.
///
/// v2 `webui` values are relative to `_links.base` (`/spaces/...`), while links
/// inside rendered bodies are site-relative (`/wiki/spaces/...`). Both resolve to
/// `https://site/wiki/spaces/...`.
fn resolve_link(wiki_base: &reqwest::Url, link: &str) -> Option<String> {
    if let Ok(url) = reqwest::Url::parse(link) {
        return Some(url.into());
    }
    if link.starts_with("//") {
        return wiki_base.join(link).ok().map(Into::into);
    }
    let base_path = wiki_base.path().trim_end_matches('/');
    let relative = match link.strip_prefix('/') {
        Some(rest) => {
            let rooted = format!("/{rest}");
            if !base_path.is_empty()
                && (rooted == base_path || rooted.starts_with(&format!("{base_path}/")))
            {
                return wiki_base.join(&rooted).ok().map(Into::into);
            }
            rest.to_string()
        }
        None => link.to_string(),
    };
    wiki_base.join(&relative).ok().map(Into::into)
}

/// Rewrite relative `href`/`src` attributes in rendered HTML to absolute URLs.
fn absolutize_links(html: &str, wiki_base: &reqwest::Url) -> String {
    let re = regex::Regex::new(r#"(?i)\b(href|src)\s*=\s*(["'])([^"']*)(["'])"#).unwrap();
    re.replace_all(html, |caps: &regex::Captures<'_>| {
        let value = &caps[3];
        let keep = value.is_empty() || value.starts_with('#') || reqwest::Url::parse(value).is_ok();
        let resolved = if keep {
            value.to_string()
        } else {
            resolve_link(wiki_base, value).unwrap_or_else(|| value.to_string())
        };
        format!("{}={}{}{}", &caps[1], &caps[2], resolved, &caps[4])
    })
    .into_owned()
}

fn page_url(
    wiki_base: &reqwest::Url,
    webui: Option<&str>,
    page_id: &str,
) -> Result<String, GetPageError> {
    if let Some(webui) = webui {
        return resolve_link(wiki_base, webui)
            .ok_or_else(|| GetPageError::InvalidResponse(format!("invalid webui link: {webui}")));
    }
    let mut url = wiki_base
        .join("pages/viewpage.action")
        .map_err(|error| GetPageError::InvalidResponse(error.to_string()))?;
    url.set_query(Some(&format!("pageId={page_id}")));
    Ok(url.into())
}

/// Convert HTML content to plain text (simple conversion)
///
/// Transforms HTML strings into readable plain text by removing tags and decoding entities.
pub fn html_to_plaintext(html: &str) -> String {
    // Simple HTML to text conversion - remove tags and decode entities
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<p>", "")
        .replace("</p>", "\n")
        .replace("<div>", "")
        .replace("</div>", "\n");

    // Remove HTML tags
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let cleaned = re.replace_all(&text, "");

    // Decode HTML entities
    let decoded = html_escape::decode_html_entities(&cleaned);

    // Clean up excessive whitespace
    decoded
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_plaintext_basic() {
        let html = "<p>Hello <strong>World</strong></p>";
        let result = html_to_plaintext(html);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_html_to_plaintext_with_breaks() {
        let html = "Line 1<br>Line 2<br/>Line 3<br />Line 4";
        let result = html_to_plaintext(html);
        assert_eq!(result, "Line 1\nLine 2\nLine 3\nLine 4");
    }

    #[test]
    fn test_html_to_plaintext_with_entities() {
        let html = "&lt;div&gt; &amp; &quot;quotes&quot;";
        let result = html_to_plaintext(html);
        assert_eq!(result, "<div> & \"quotes\"");
    }

    #[test]
    fn test_html_to_plaintext_removes_excess_whitespace() {
        let html = "<p>  Too   much    space  </p>";
        let result = html_to_plaintext(html);
        assert_eq!(result, "Too   much    space");
    }

    #[test]
    fn build_search_request_uses_v1_cql_endpoint() {
        let request = build_search_request(
            "https://example.atlassian.net/wiki/",
            "text ~ 'deployment'",
            10,
        )
        .unwrap();
        let url = reqwest::Url::parse(&request.url).unwrap();
        assert_eq!(url.path(), "/wiki/rest/api/search");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("cql".into(), "text ~ 'deployment'".into()),
                ("limit".into(), "10".into()),
                ("expand".into(), "content.body.view".into()),
            ]
        );
    }

    #[test]
    fn parse_search_response_reads_nested_v1_content() {
        let response = r#"{
            "results": [{
                "content": {
                    "id": "123",
                    "title": "Deployment Runbook",
                    "type": "page",
                    "body": { "view": { "value": "<p>Deploy &amp; verify</p>" } },
                    "_links": { "webui": "/wiki/spaces/ENG/pages/123" }
                },
                "url": "/wiki/spaces/ENG/pages/123/Deployment-Runbook"
            }],
            "size": 1,
            "totalSize": 4
        }"#;
        let output = parse_search_response(response).unwrap();
        assert_eq!(output.total, 4);
        assert_eq!(
            output.pages,
            vec![PageOutput {
                id: "123".to_string(),
                title: "Deployment Runbook".to_string(),
                page_type: "page".to_string(),
                url: Some("/wiki/spaces/ENG/pages/123/Deployment-Runbook".to_string()),
                content: Some("Deploy & verify".to_string()),
            }]
        );
    }

    #[test]
    fn parse_search_response_rejects_v2_page_shape() {
        let response = r#"{
            "results": [{
                "id": "123",
                "title": "Deployment Runbook",
                "type": "page",
                "_links": {}
            }]
        }"#;
        assert!(matches!(
            parse_search_response(response),
            Err(SearchError::InvalidResponse(_))
        ));
    }

    #[test]
    fn build_get_page_request_accepts_id_and_cloud_url() {
        let id = build_get_page_request("https://example.atlassian.net", "12345").unwrap();
        assert_eq!(id.page_id, "12345");
        assert_eq!(
            id.url,
            "https://example.atlassian.net/wiki/api/v2/pages/12345?body-format=view"
        );

        let url = build_get_page_request(
            "https://example.atlassian.net/wiki/",
            "https://example.atlassian.net/wiki/spaces/ENG/pages/67890/Page+Title",
        )
        .unwrap();
        assert_eq!(url.page_id, "67890");
        assert_eq!(
            url.url,
            "https://example.atlassian.net/wiki/api/v2/pages/67890?body-format=view"
        );
    }

    #[test]
    fn build_get_page_request_accepts_page_id_query_url() {
        let request = build_get_page_request(
            "https://example.atlassian.net",
            "https://example.atlassian.net/wiki/pages/viewpage.action?pageId=24680",
        )
        .unwrap();
        assert_eq!(request.page_id, "24680");
    }

    #[test]
    fn build_get_page_request_rejects_url_without_page_id() {
        assert_eq!(
            build_get_page_request(
                "https://example.atlassian.net",
                "https://example.atlassian.net/wiki/spaces/ENG/overview",
            ),
            Err(GetPageError::InvalidPageReference)
        );
    }

    #[test]
    fn parse_get_page_response_converts_view_html_to_markdown() {
        let response = r#"{
            "id": "12345",
            "title": "Runbook",
            "spaceId": "9988",
            "version": { "number": 7 },
            "body": {
                "view": {
                    "representation": "view",
                    "value": "<h1>Deploy</h1><p>Run <strong>checks</strong>.</p>"
                }
            },
            "_links": {
                "webui": "/wiki/spaces/ENG/pages/12345/Runbook"
            }
        }"#;
        let output = parse_get_page_response(response, "https://example.atlassian.net").unwrap();
        assert_eq!(output.id, "12345");
        assert_eq!(output.title, "Runbook");
        assert_eq!(output.space_id, "9988");
        assert_eq!(output.version, 7);
        assert!(output.body.contains("Deploy"));
        assert!(output.body.contains("**checks**"));
        assert!(!output.body.contains("<h1>"));
        assert_eq!(
            output.url,
            "https://example.atlassian.net/wiki/spaces/ENG/pages/12345/Runbook"
        );
    }

    #[test]
    fn parse_get_page_response_resolves_webui_relative_to_links_base() {
        let response = r#"{
            "id": "3422322689",
            "title": "2026-10-08",
            "spaceId": "1",
            "version": { "number": 1 },
            "body": { "view": { "value": "<p>hi</p>" } },
            "_links": {
                "webui": "/spaces/VP20/pages/3422322689/2026-10-08",
                "base": "https://vntana.atlassian.net/wiki"
            }
        }"#;
        let output = parse_get_page_response(response, "https://vntana.atlassian.net").unwrap();
        assert_eq!(
            output.url,
            "https://vntana.atlassian.net/wiki/spaces/VP20/pages/3422322689/2026-10-08"
        );
    }

    #[test]
    fn parse_get_page_response_adds_wiki_without_links_base() {
        let response = r#"{
            "id": "42",
            "title": "T",
            "spaceId": "1",
            "version": { "number": 1 },
            "body": { "view": { "value": "<p>hi</p>" } },
            "_links": { "webui": "/spaces/ENG/pages/42/T" }
        }"#;
        let output = parse_get_page_response(response, "https://example.atlassian.net/").unwrap();
        assert_eq!(
            output.url,
            "https://example.atlassian.net/wiki/spaces/ENG/pages/42/T"
        );
    }

    #[test]
    fn parse_get_page_response_makes_body_links_absolute() {
        let response = r##"{
            "id": "42",
            "title": "T",
            "spaceId": "1",
            "version": { "number": 1 },
            "body": { "view": { "value": "<p><a href=\"/wiki/spaces/ENG/pages/7/Other\">Other</a> <a href=\"https://x.test/a\">Ext</a> <a href=\"#top\">Top</a></p>" } },
            "_links": {
                "webui": "/spaces/ENG/pages/42/T",
                "base": "https://example.atlassian.net/wiki"
            }
        }"##;
        let output = parse_get_page_response(response, "https://example.atlassian.net").unwrap();
        assert!(
            output
                .body
                .contains("(https://example.atlassian.net/wiki/spaces/ENG/pages/7/Other)"),
            "{}",
            output.body
        );
        assert!(output.body.contains("(https://x.test/a)"));
        assert!(output.body.contains("(#top)"));
        assert!(!output.body.contains("](/wiki"));
    }

    #[test]
    fn build_create_page_request_uses_v2_storage_body_and_parent_id() {
        let request = build_create_page_request(
            "https://example.atlassian.net/wiki/",
            "9988",
            "Runbook",
            "<p>Deploy</p>",
            Some("https://example.atlassian.net/wiki/spaces/ENG/pages/42/Parent"),
        )
        .unwrap();
        assert_eq!(
            request.url,
            "https://example.atlassian.net/wiki/api/v2/pages"
        );
        assert_eq!(
            request.body,
            serde_json::json!({
                "spaceId": "9988",
                "status": "current",
                "title": "Runbook",
                "body": {"representation": "storage", "value": "<p>Deploy</p>"},
                "parentId": "42"
            })
        );
    }

    #[test]
    fn build_update_page_request_increments_version_and_preserves_unchanged_body() {
        let source_response = r#"{
            "id": "12345",
            "title": "Old title",
            "spaceId": "9988",
            "version": {"number": 7},
            "body": {"storage": {"value": "<p>Existing</p>"}}
        }"#;
        let source = parse_update_page_source(source_response).unwrap();
        let changes = update_page_changes(Some("New title".to_string()), None).unwrap();
        let request =
            build_update_page_request("https://example.atlassian.net", "12345", &source, &changes)
                .unwrap();
        assert_eq!(
            request.url,
            "https://example.atlassian.net/wiki/api/v2/pages/12345"
        );
        assert_eq!(
            request.body,
            serde_json::json!({
                "id": "12345",
                "status": "current",
                "title": "New title",
                "body": {"representation": "storage", "value": "<p>Existing</p>"},
                "version": {"number": 8}
            })
        );
    }

    #[test]
    fn build_update_page_source_request_accepts_url_and_requests_storage() {
        let request = build_update_page_source_request(
            "https://example.atlassian.net",
            "https://example.atlassian.net/wiki/pages/viewpage.action?pageId=24680",
        )
        .unwrap();
        assert_eq!(request.page_id, "24680");
        assert_eq!(
            request.url,
            "https://example.atlassian.net/wiki/api/v2/pages/24680?body-format=storage"
        );
    }

    #[test]
    fn update_page_changes_requires_title_or_body() {
        assert_eq!(
            update_page_changes(None, None),
            Err(WritePageError::NoChanges)
        );
        assert!(update_page_changes(None, Some(String::new())).is_ok());
    }

    #[test]
    fn parse_write_page_response_builds_web_url() {
        let response = r#"{
            "id": "12345",
            "title": "Runbook",
            "spaceId": "9988",
            "version": {"number": 8},
            "_links": {
                "base": "https://example.atlassian.net/wiki",
                "webui": "/spaces/ENG/pages/12345/Runbook"
            }
        }"#;
        let output = parse_write_page_response(response, "https://example.atlassian.net").unwrap();
        assert_eq!(
            output,
            WritePageOutput {
                id: "12345".to_string(),
                title: "Runbook".to_string(),
                space_id: "9988".to_string(),
                version: 8,
                url: "https://example.atlassian.net/wiki/spaces/ENG/pages/12345/Runbook"
                    .to_string(),
            }
        );
    }
}
