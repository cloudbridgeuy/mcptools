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

/// Search response from Confluence API
#[derive(Debug, Deserialize, Clone)]
pub struct ConfluenceSearchResponse {
    pub results: Vec<ConfluencePageResponse>,
    #[serde(default)]
    pub size: usize,
    #[serde(default, rename = "totalSize")]
    pub total_size: usize,
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

fn page_id_from_reference(id_or_url: &str) -> Result<String, GetPageError> {
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

/// Convert Confluence API response to domain model
///
/// Transforms the raw API response into our clean domain model.
///
/// # Arguments
/// * `search_response` - The raw response from Confluence search API
///
/// # Returns
/// * `SearchOutput` - Cleaned and transformed search results
pub fn transform_search_results(search_response: ConfluenceSearchResponse) -> SearchOutput {
    let pages = search_response
        .results
        .into_iter()
        .map(|page| {
            let content = page
                .body
                .as_ref()
                .and_then(|b| b.view.as_ref())
                .and_then(|v| v.value.as_ref())
                .map(|html| html_to_plaintext(html));

            PageOutput {
                id: page.id,
                title: page.title,
                page_type: page.page_type,
                url: page.links.webui,
                content,
            }
        })
        .collect();

    SearchOutput {
        pages,
        total: search_response.total_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a basic page response for testing
    fn create_page_response(
        id: &str,
        title: &str,
        page_type: &str,
        webui: Option<String>,
        html_content: Option<String>,
    ) -> ConfluencePageResponse {
        ConfluencePageResponse {
            id: id.to_string(),
            title: title.to_string(),
            page_type: page_type.to_string(),
            status: Some("current".to_string()),
            links: PageLinks { webui },
            body: html_content.map(|html| PageBody {
                view: Some(ViewContent { value: Some(html) }),
            }),
        }
    }

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
    fn test_transform_search_results_basic() {
        // Arrange: Create a basic search response with one page
        let response = ConfluenceSearchResponse {
            results: vec![create_page_response(
                "123",
                "Test Page",
                "page",
                Some("https://example.com/test".to_string()),
                Some("<p>Test content</p>".to_string()),
            )],
            size: 1,
            total_size: 1,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify the transformation
        assert_eq!(output.total, 1);
        assert_eq!(output.pages.len(), 1);

        let page = &output.pages[0];
        assert_eq!(page.id, "123");
        assert_eq!(page.title, "Test Page");
        assert_eq!(page.page_type, "page");
        assert_eq!(page.url, Some("https://example.com/test".to_string()));
        assert_eq!(page.content, Some("Test content".to_string()));
    }

    #[test]
    fn test_transform_search_results_empty() {
        // Arrange: Create an empty search response
        let response = ConfluenceSearchResponse {
            results: vec![],
            size: 0,
            total_size: 0,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify empty results
        assert_eq!(output.total, 0);
        assert_eq!(output.pages.len(), 0);
    }

    #[test]
    fn test_transform_search_results_missing_content() {
        // Arrange: Create a page without body content
        let response = ConfluenceSearchResponse {
            results: vec![create_page_response(
                "456",
                "Empty Page",
                "page",
                Some("https://example.com/empty".to_string()),
                None, // No content
            )],
            size: 1,
            total_size: 1,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify page with no content
        assert_eq!(output.pages.len(), 1);
        let page = &output.pages[0];
        assert_eq!(page.id, "456");
        assert_eq!(page.content, None);
    }

    #[test]
    fn test_transform_search_results_missing_webui() {
        // Arrange: Create a page without webui link
        let response = ConfluenceSearchResponse {
            results: vec![create_page_response(
                "789",
                "No URL Page",
                "page",
                None, // No webui link
                Some("<p>Content here</p>".to_string()),
            )],
            size: 1,
            total_size: 1,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify page with no URL
        assert_eq!(output.pages.len(), 1);
        let page = &output.pages[0];
        assert_eq!(page.url, None);
        assert_eq!(page.content, Some("Content here".to_string()));
    }

    #[test]
    fn test_transform_search_results_multiple_pages() {
        // Arrange: Create a search response with multiple pages
        let response = ConfluenceSearchResponse {
            results: vec![
                create_page_response(
                    "1",
                    "Page One",
                    "page",
                    Some("https://example.com/1".to_string()),
                    Some("<p>Content 1</p>".to_string()),
                ),
                create_page_response(
                    "2",
                    "Page Two",
                    "blogpost",
                    Some("https://example.com/2".to_string()),
                    Some("<p>Content 2</p>".to_string()),
                ),
                create_page_response("3", "Page Three", "page", None, None),
            ],
            size: 3,
            total_size: 3,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify all pages transformed correctly
        assert_eq!(output.total, 3);
        assert_eq!(output.pages.len(), 3);

        // Check first page
        assert_eq!(output.pages[0].id, "1");
        assert_eq!(output.pages[0].title, "Page One");
        assert_eq!(output.pages[0].page_type, "page");

        // Check second page (blogpost type)
        assert_eq!(output.pages[1].id, "2");
        assert_eq!(output.pages[1].page_type, "blogpost");

        // Check third page (no URL or content)
        assert_eq!(output.pages[2].id, "3");
        assert_eq!(output.pages[2].url, None);
        assert_eq!(output.pages[2].content, None);
    }

    #[test]
    fn test_transform_search_results_complex_html() {
        // Arrange: Create a page with complex HTML content
        let complex_html = r#"
            <div>
                <h1>Title</h1>
                <p>First paragraph with <strong>bold</strong> and <em>italic</em>.</p>
                <ul>
                    <li>Item 1</li>
                    <li>Item 2</li>
                </ul>
                <p>Second paragraph with &amp; special &lt;chars&gt;.</p>
            </div>
        "#;

        let response = ConfluenceSearchResponse {
            results: vec![create_page_response(
                "complex",
                "Complex Page",
                "page",
                Some("https://example.com/complex".to_string()),
                Some(complex_html.to_string()),
            )],
            size: 1,
            total_size: 1,
        };

        // Act: Transform the response
        let output = transform_search_results(response);

        // Assert: Verify HTML was converted to clean text
        assert_eq!(output.pages.len(), 1);
        let content = output.pages[0].content.as_ref().unwrap();

        // Should contain the text but not the HTML tags
        assert!(content.contains("Title"));
        assert!(content.contains("First paragraph with bold and italic"));
        assert!(content.contains("Item 1"));
        assert!(content.contains("Item 2"));
        assert!(content.contains("& special <chars>"));

        // Should not contain HTML tags
        assert!(!content.contains("<h1>"));
        assert!(!content.contains("<p>"));
        assert!(!content.contains("<ul>"));
        assert!(!content.contains("&amp;"));
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
}
