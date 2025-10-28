//! Pure transformation functions for Confluence API responses
//!
//! This module contains zero I/O operations and is fully testable with fixture data.

use serde::{Deserialize, Serialize};

// ============================================================================
// Domain Models (Input from API)
// ============================================================================

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

// ============================================================================
// Output Models (Domain Model)
// ============================================================================

/// Output structure for a single page
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PageOutput {
    pub id: String,
    pub title: String,
    pub page_type: String,
    pub url: Option<String>,
    pub content: Option<String>,
}

/// Output structure for search command
#[derive(Debug, Serialize, PartialEq)]
pub struct SearchOutput {
    pub pages: Vec<PageOutput>,
    pub total: usize,
}

// ============================================================================
// Pure Helper Functions
// ============================================================================

/// Convert HTML content to plain text (simple conversion)
///
/// This is a pure function with no side effects. It transforms HTML strings
/// into readable plain text by removing tags and decoding entities.
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

// ============================================================================
// Pure Transformation Functions
// ============================================================================

/// Pure transformation: Convert Confluence API response to domain model
///
/// This function has no side effects and can be tested without mocking HTTP.
/// It transforms the raw API response into our clean domain model.
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

// ============================================================================
// Tests
// ============================================================================

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
}
