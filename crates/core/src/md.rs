use regex::Regex;
use scraper::{Html, Selector as CssSelector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionStrategy {
    First,
    Last,
    All,
    N,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdPaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_characters: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchOutput {
    pub url: String,
    pub title: Option<String>,
    pub content: String,
    pub html_length: usize,
    pub fetch_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements_found: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_applied: Option<String>,
    pub pagination: MdPaginationInfo,
}

#[derive(Debug, Clone)]
pub struct ProcessedContent {
    pub content: String,
    pub selector_used: Option<String>,
    pub elements_found: Option<usize>,
    pub strategy_applied: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PaginationResult {
    pub start_offset: usize,
    pub end_offset: usize,
    pub current_page: usize,
    pub pagination_info: MdPaginationInfo,
}

/// Remove script and style tags from HTML
pub fn clean_html(html: &str) -> String {
    let script_regex = Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap();
    let html = script_regex.replace_all(html, "");

    let style_regex = Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap();
    let html = style_regex.replace_all(&html, "");

    html.to_string()
}

/// Apply CSS selector to HTML and return filtered HTML and count of elements found
pub fn apply_selector(
    html: &str,
    selector_str: &str,
    strategy: &SelectionStrategy,
    index: Option<usize>,
) -> Result<(String, usize), String> {
    let document = Html::parse_document(html);

    let selector = CssSelector::parse(selector_str)
        .map_err(|e| format!("Invalid CSS selector '{selector_str}': {e:?}"))?;

    let elements: Vec<_> = document.select(&selector).collect();
    let count = elements.len();

    if count == 0 {
        return Err(format!(
            "No elements found matching selector: '{selector_str}'"
        ));
    }

    let selected_html = match strategy {
        SelectionStrategy::First => elements
            .first()
            .map(|el| el.html())
            .ok_or_else(|| "No first element found".to_string())?,
        SelectionStrategy::Last => elements
            .last()
            .map(|el| el.html())
            .ok_or_else(|| "No last element found".to_string())?,
        SelectionStrategy::All => elements
            .iter()
            .map(|el| el.html())
            .collect::<Vec<_>>()
            .join("\n"),
        SelectionStrategy::N => {
            let idx = index.ok_or_else(|| "Index required for 'n' strategy".to_string())?;
            elements
                .get(idx)
                .map(|el| el.html())
                .ok_or_else(|| format!("Index {idx} out of bounds (found {count} elements)"))?
        }
    };

    Ok((selected_html, count))
}

/// Process HTML content with optional CSS selector filtering and conversion to markdown
pub fn process_html_content(
    html: String,
    selector: Option<String>,
    strategy: SelectionStrategy,
    index: Option<usize>,
    raw_html: bool,
) -> Result<ProcessedContent, String> {
    let (filtered_html, selector_used, elements_found, strategy_applied) =
        if let Some(ref sel) = selector {
            let (filtered, count) = apply_selector(&html, sel, &strategy, index)?;
            let strategy_desc = match strategy {
                SelectionStrategy::First => "first".to_string(),
                SelectionStrategy::Last => "last".to_string(),
                SelectionStrategy::All => "all".to_string(),
                SelectionStrategy::N => format!("nth (index: {})", index.unwrap_or(0)),
            };
            (
                filtered,
                Some(sel.clone()),
                Some(count),
                Some(strategy_desc),
            )
        } else {
            (html, None, None, None)
        };

    let cleaned_html = clean_html(&filtered_html);

    let content = if raw_html {
        cleaned_html
    } else {
        html2md::parse_html(&cleaned_html)
    };

    Ok(ProcessedContent {
        content,
        selector_used,
        elements_found,
        strategy_applied,
    })
}

/// Calculate pagination for content based on offset, limit, and page parameters
pub fn calculate_pagination(
    total_characters: usize,
    offset: usize,
    limit: usize,
    page: usize,
) -> PaginationResult {
    let (total_pages, start_offset, end_offset, current_page) = if offset > 0 {
        // Offset-based: ignore page parameter
        let start_offset = offset.min(total_characters);
        let end_offset = (start_offset + limit).min(total_characters);
        let total_pages = if limit >= total_characters {
            1
        } else {
            total_characters.div_ceil(limit)
        };
        let current_page = if limit > 0 { (offset / limit) + 1 } else { 1 };
        (total_pages, start_offset, end_offset, current_page)
    } else if limit >= total_characters {
        // Single page case
        (1, 0, total_characters, 1)
    } else {
        // Multi-page case
        let total_pages = total_characters.div_ceil(limit);
        let current_page = page.min(total_pages.max(1));
        let start_offset = (current_page - 1) * limit;
        let end_offset = (start_offset + limit).min(total_characters);
        (total_pages, start_offset, end_offset, current_page)
    };

    let has_more = current_page < total_pages;

    let pagination_info = MdPaginationInfo {
        current_page,
        total_pages,
        total_characters,
        limit,
        has_more,
    };

    PaginationResult {
        start_offset,
        end_offset,
        current_page,
        pagination_info,
    }
}

/// Extract paginated content from full content using character offsets
pub fn slice_content(content: String, start_offset: usize, end_offset: usize) -> String {
    content
        .chars()
        .skip(start_offset)
        .take(end_offset - start_offset)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html_removes_script_tags() {
        let html = r#"<div>Content</div><script>alert('hi');</script><p>More</p>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<script"));
        assert!(!cleaned.contains("alert"));
        assert!(cleaned.contains("<div>Content</div>"));
        assert!(cleaned.contains("<p>More</p>"));
    }

    #[test]
    fn test_clean_html_removes_style_tags() {
        let html = r#"<div>Content</div><style>.class { color: red; }</style><p>More</p>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<style"));
        assert!(!cleaned.contains("color: red"));
        assert!(cleaned.contains("<div>Content</div>"));
        assert!(cleaned.contains("<p>More</p>"));
    }

    #[test]
    fn test_clean_html_removes_both() {
        let html = r#"<div>Content</div><script>alert('hi');</script><style>.class { color: red; }</style><p>More</p>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<script"));
        assert!(!cleaned.contains("<style"));
        assert!(cleaned.contains("<div>Content</div>"));
        assert!(cleaned.contains("<p>More</p>"));
    }

    #[test]
    fn test_apply_selector_first_strategy() {
        let html = r#"<div class="item">First</div><div class="item">Second</div>"#;
        let (result, count) =
            apply_selector(html, ".item", &SelectionStrategy::First, None).unwrap();
        assert_eq!(count, 2);
        assert!(result.contains("First"));
        assert!(!result.contains("Second"));
    }

    #[test]
    fn test_apply_selector_last_strategy() {
        let html = r#"<div class="item">First</div><div class="item">Second</div>"#;
        let (result, count) =
            apply_selector(html, ".item", &SelectionStrategy::Last, None).unwrap();
        assert_eq!(count, 2);
        assert!(!result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_apply_selector_all_strategy() {
        let html = r#"<div class="item">First</div><div class="item">Second</div>"#;
        let (result, count) = apply_selector(html, ".item", &SelectionStrategy::All, None).unwrap();
        assert_eq!(count, 2);
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_apply_selector_n_strategy() {
        let html = r#"<div class="item">First</div><div class="item">Second</div><div class="item">Third</div>"#;
        let (result, count) =
            apply_selector(html, ".item", &SelectionStrategy::N, Some(1)).unwrap();
        assert_eq!(count, 3);
        assert!(result.contains("Second"));
        assert!(!result.contains("First"));
        assert!(!result.contains("Third"));
    }

    #[test]
    fn test_apply_selector_no_matches() {
        let html = r#"<div class="item">Content</div>"#;
        let result = apply_selector(html, ".nonexistent", &SelectionStrategy::First, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No elements found"));
    }

    #[test]
    fn test_apply_selector_invalid_selector() {
        let html = r#"<div>Content</div>"#;
        let result = apply_selector(html, "::invalid::selector", &SelectionStrategy::First, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_selector_n_out_of_bounds() {
        let html = r#"<div class="item">First</div><div class="item">Second</div>"#;
        let result = apply_selector(html, ".item", &SelectionStrategy::N, Some(5));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of bounds"));
    }

    #[test]
    fn test_apply_selector_n_missing_index() {
        let html = r#"<div class="item">First</div>"#;
        let result = apply_selector(html, ".item", &SelectionStrategy::N, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Index required"));
    }

    #[test]
    fn test_process_html_content_without_selector() {
        let html = r#"<div>Content</div>"#.to_string();
        let result =
            process_html_content(html, None, SelectionStrategy::First, None, false).unwrap();

        assert!(result.content.contains("Content"));
        assert!(result.selector_used.is_none());
        assert!(result.elements_found.is_none());
        assert!(result.strategy_applied.is_none());
    }

    #[test]
    fn test_process_html_content_with_selector() {
        let html = r#"<div class="content">Main</div><div>Other</div>"#.to_string();
        let result = process_html_content(
            html,
            Some(".content".to_string()),
            SelectionStrategy::First,
            None,
            false,
        )
        .unwrap();

        assert!(result.content.contains("Main"));
        assert_eq!(result.selector_used, Some(".content".to_string()));
        assert_eq!(result.elements_found, Some(1));
        assert_eq!(result.strategy_applied, Some("first".to_string()));
    }

    #[test]
    fn test_process_html_content_raw_html() {
        let html = r#"<div><p>Content</p></div>"#.to_string();
        let result =
            process_html_content(html, None, SelectionStrategy::First, None, true).unwrap();

        assert!(result.content.contains("<p>Content</p>"));
    }

    #[test]
    fn test_process_html_content_removes_scripts() {
        let html = r#"<div>Content</div><script>alert('hi');</script>"#.to_string();
        let result =
            process_html_content(html, None, SelectionStrategy::First, None, false).unwrap();

        assert!(!result.content.contains("alert"));
        assert!(result.content.contains("Content"));
    }

    #[test]
    fn test_calculate_pagination_single_page() {
        let result = calculate_pagination(100, 0, 1000, 1);
        assert_eq!(result.start_offset, 0);
        assert_eq!(result.end_offset, 100);
        assert_eq!(result.current_page, 1);
        assert_eq!(result.pagination_info.total_pages, 1);
        assert!(!result.pagination_info.has_more);
    }

    #[test]
    fn test_calculate_pagination_multi_page_first() {
        let result = calculate_pagination(1000, 0, 100, 1);
        assert_eq!(result.start_offset, 0);
        assert_eq!(result.end_offset, 100);
        assert_eq!(result.current_page, 1);
        assert_eq!(result.pagination_info.total_pages, 10);
        assert!(result.pagination_info.has_more);
    }

    #[test]
    fn test_calculate_pagination_multi_page_middle() {
        let result = calculate_pagination(1000, 0, 100, 5);
        assert_eq!(result.start_offset, 400);
        assert_eq!(result.end_offset, 500);
        assert_eq!(result.current_page, 5);
        assert_eq!(result.pagination_info.total_pages, 10);
        assert!(result.pagination_info.has_more);
    }

    #[test]
    fn test_calculate_pagination_multi_page_last() {
        let result = calculate_pagination(1000, 0, 100, 10);
        assert_eq!(result.start_offset, 900);
        assert_eq!(result.end_offset, 1000);
        assert_eq!(result.current_page, 10);
        assert_eq!(result.pagination_info.total_pages, 10);
        assert!(!result.pagination_info.has_more);
    }

    #[test]
    fn test_calculate_pagination_offset_based() {
        let result = calculate_pagination(1000, 250, 100, 999);
        assert_eq!(result.start_offset, 250);
        assert_eq!(result.end_offset, 350);
        assert_eq!(result.current_page, 3);
        assert_eq!(result.pagination_info.total_pages, 10);
    }

    #[test]
    fn test_calculate_pagination_offset_beyond_end() {
        let result = calculate_pagination(100, 150, 50, 1);
        assert_eq!(result.start_offset, 100);
        assert_eq!(result.end_offset, 100);
        assert_eq!(result.pagination_info.total_characters, 100);
    }

    #[test]
    fn test_calculate_pagination_empty_content() {
        let result = calculate_pagination(0, 0, 100, 1);
        assert_eq!(result.start_offset, 0);
        assert_eq!(result.end_offset, 0);
        assert_eq!(result.current_page, 1);
        assert_eq!(result.pagination_info.total_pages, 1);
    }

    #[test]
    fn test_calculate_pagination_page_out_of_bounds() {
        let result = calculate_pagination(1000, 0, 100, 999);
        assert_eq!(result.current_page, 10);
        assert_eq!(result.start_offset, 900);
        assert_eq!(result.end_offset, 1000);
    }

    #[test]
    fn test_slice_content_basic() {
        let content = "Hello, World!".to_string();
        let sliced = slice_content(content, 0, 5);
        assert_eq!(sliced, "Hello");
    }

    #[test]
    fn test_slice_content_middle() {
        let content = "Hello, World!".to_string();
        let sliced = slice_content(content, 7, 12);
        assert_eq!(sliced, "World");
    }

    #[test]
    fn test_slice_content_unicode() {
        let content = "Hello 世界!".to_string();
        let sliced = slice_content(content, 6, 8);
        assert_eq!(sliced, "世界");
    }

    #[test]
    fn test_slice_content_empty() {
        let content = "Hello".to_string();
        let sliced = slice_content(content, 5, 5);
        assert_eq!(sliced, "");
    }

    #[test]
    fn test_slice_content_full() {
        let content = "Hello, World!".to_string();
        let len = content.chars().count();
        let sliced = slice_content(content.clone(), 0, len);
        assert_eq!(sliced, content);
    }
}
