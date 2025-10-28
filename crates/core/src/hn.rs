use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// HackerNews item from API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HnItem {
    pub id: u64,
    #[serde(rename = "type")]
    pub item_type: String,
    pub by: Option<String>,
    pub time: Option<u64>,
    pub text: Option<String>,
    pub dead: Option<bool>,
    pub deleted: Option<bool>,
    pub parent: Option<u64>,
    pub kids: Option<Vec<u64>>,
    pub url: Option<String>,
    pub score: Option<u64>,
    pub title: Option<String>,
    pub descendants: Option<u64>,
}

/// Individual list item output
#[derive(Debug, Serialize, Clone)]
pub struct ListItem {
    pub id: u64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub score: Option<u64>,
    pub time: Option<String>,
    pub comments: Option<u64>,
}

/// Pagination metadata for list output
#[derive(Debug, Serialize, Clone)]
pub struct ListPaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_items: usize,
    pub limit: usize,
    pub next_page_command: Option<String>,
    pub prev_page_command: Option<String>,
}

/// Complete list output with items and pagination
#[derive(Debug, Serialize, Clone)]
pub struct ListOutput {
    pub story_type: String,
    pub items: Vec<ListItem>,
    pub pagination: ListPaginationInfo,
}

/// Post output with comments and pagination
#[derive(Debug, Serialize, Clone)]
pub struct PostOutput {
    pub id: u64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub score: Option<u64>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub total_comments: Option<u64>,
    pub comments: Vec<CommentOutput>,
    pub pagination: PaginationInfo,
}

/// Individual comment output
#[derive(Debug, Serialize, Clone)]
pub struct CommentOutput {
    pub id: u64,
    pub author: Option<String>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub replies_count: usize,
}

/// Pagination metadata for post reading
#[derive(Debug, Serialize, Clone)]
pub struct PaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_comments: usize,
    pub limit: usize,
    pub next_page_command: Option<String>,
    pub prev_page_command: Option<String>,
}

/// Convert Unix timestamp to formatted string
pub fn format_timestamp(timestamp: Option<u64>) -> Option<String> {
    timestamp.and_then(|ts| {
        let dt = DateTime::<Utc>::from_timestamp(ts as i64, 0)?;
        Some(dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
    })
}

/// Calculate pagination bounds for a given page
///
/// Returns (start_index, end_index) for slicing the items array.
/// Returns an error if the page is out of range or if there are no items.
pub fn calculate_pagination(
    total_items: usize,
    page: usize,
    limit: usize,
) -> Result<(usize, usize), String> {
    if total_items == 0 {
        return Err("No items available for pagination".to_string());
    }

    let start = (page - 1) * limit;

    if start >= total_items {
        let total_pages = total_items.div_ceil(limit);
        return Err(format!(
            "Page {page} is out of range. Only {total_pages} pages available."
        ));
    }

    let end = (start + limit).min(total_items);
    Ok((start, end))
}

/// Transform HackerNews items into list output with pagination
///
/// Takes raw HN API items and constructs a complete ListOutput with:
/// - Transformed list items with formatted timestamps
/// - Pagination metadata
/// - Navigation commands
pub fn transform_hn_items(
    items: Vec<HnItem>,
    story_type: String,
    page: usize,
    limit: usize,
    total_items: usize,
) -> ListOutput {
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| ListItem {
            id: item.id,
            title: item.title.clone(),
            url: item.url.clone(),
            author: item.by.clone(),
            score: item.score,
            time: format_timestamp(item.time),
            comments: item.descendants,
        })
        .collect();

    let total_pages = total_items.div_ceil(limit);

    let next_page = if page < total_pages {
        Some(format!(
            "mcptools hn list {} --page {}",
            story_type,
            page + 1
        ))
    } else {
        None
    };

    let prev_page = if page > 1 {
        Some(format!(
            "mcptools hn list {} --page {}",
            story_type,
            page - 1
        ))
    } else {
        None
    };

    ListOutput {
        story_type,
        items: list_items,
        pagination: ListPaginationInfo {
            current_page: page,
            total_pages,
            total_items,
            limit,
            next_page_command: next_page,
            prev_page_command: prev_page,
        },
    }
}

/// Strip HTML tags and decode HTML entities from text
///
/// Removes all HTML tags and decodes common HTML entities to their
/// plain text equivalents.
pub fn strip_html(text: &str) -> String {
    let re = Regex::new(r"<[^>]*>").unwrap();
    let stripped = re.replace_all(text, "");
    stripped
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("<p>", "\n")
}

/// Transform HN items to comment outputs
///
/// Converts raw HN API items into structured comment outputs with
/// formatted timestamps and cleaned text.
pub fn transform_comments(comments: Vec<HnItem>) -> Vec<CommentOutput> {
    comments
        .iter()
        .map(|c| CommentOutput {
            id: c.id,
            author: c.by.clone(),
            time: format_timestamp(c.time),
            text: c.text.as_ref().map(|t| strip_html(t)),
            replies_count: c.kids.as_ref().map(|k| k.len()).unwrap_or(0),
        })
        .collect()
}

/// Build post output with pagination metadata
///
/// Constructs a complete post output including the post details,
/// comments, and pagination information with navigation commands.
pub fn build_post_output(
    item: HnItem,
    comments: Vec<CommentOutput>,
    page: usize,
    limit: usize,
    total_comments: usize,
) -> PostOutput {
    let total_pages = total_comments.div_ceil(limit);

    let next_page = if page < total_pages {
        Some(format!("mcptools hn read {} --page {}", item.id, page + 1))
    } else {
        None
    };

    let prev_page = if page > 1 {
        Some(format!("mcptools hn read {} --page {}", item.id, page - 1))
    } else {
        None
    };

    PostOutput {
        id: item.id,
        title: item.title.clone(),
        url: item.url.clone(),
        author: item.by.clone(),
        score: item.score,
        time: format_timestamp(item.time),
        text: item.text.as_ref().map(|t| strip_html(t)),
        total_comments: item.descendants,
        comments,
        pagination: PaginationInfo {
            current_page: page,
            total_pages,
            total_comments,
            limit,
            next_page_command: next_page,
            prev_page_command: prev_page,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_valid() {
        let timestamp = Some(1609459200); // 2021-01-01 00:00:00 UTC
        let formatted = format_timestamp(timestamp);
        assert_eq!(formatted, Some("2021-01-01 00:00:00 UTC".to_string()));
    }

    #[test]
    fn test_format_timestamp_none() {
        let formatted = format_timestamp(None);
        assert_eq!(formatted, None);
    }

    #[test]
    fn test_calculate_pagination_basic() {
        let (start, end) = calculate_pagination(100, 2, 10).unwrap();
        assert_eq!(start, 10);
        assert_eq!(end, 20);
    }

    #[test]
    fn test_calculate_pagination_first_page() {
        let (start, end) = calculate_pagination(100, 1, 10).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 10);
    }

    #[test]
    fn test_calculate_pagination_last_page() {
        let (start, end) = calculate_pagination(95, 10, 10).unwrap();
        assert_eq!(start, 90);
        assert_eq!(end, 95);
    }

    #[test]
    fn test_calculate_pagination_out_of_bounds() {
        let result = calculate_pagination(100, 20, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Page 20 is out of range"));
    }

    #[test]
    fn test_calculate_pagination_empty() {
        let result = calculate_pagination(0, 1, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No items available"));
    }

    #[test]
    fn test_calculate_pagination_single_page() {
        let (start, end) = calculate_pagination(5, 1, 10).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_calculate_pagination_exact_boundary() {
        let (start, end) = calculate_pagination(100, 10, 10).unwrap();
        assert_eq!(start, 90);
        assert_eq!(end, 100);
    }

    #[test]
    fn test_transform_hn_items_single_item() {
        let items = vec![HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("testuser".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: Some("https://example.com".to_string()),
            score: Some(100),
            title: Some("Test Story".to_string()),
            descendants: Some(42),
        }];

        let output = transform_hn_items(items, "top".to_string(), 1, 10, 1);

        assert_eq!(output.story_type, "top");
        assert_eq!(output.items.len(), 1);
        assert_eq!(output.items[0].id, 12345);
        assert_eq!(output.items[0].title, Some("Test Story".to_string()));
        assert_eq!(output.items[0].author, Some("testuser".to_string()));
        assert_eq!(output.items[0].score, Some(100));
        assert_eq!(output.items[0].comments, Some(42));
        assert_eq!(output.pagination.current_page, 1);
        assert_eq!(output.pagination.total_pages, 1);
        assert_eq!(output.pagination.total_items, 1);
        assert!(output.pagination.next_page_command.is_none());
        assert!(output.pagination.prev_page_command.is_none());
    }

    #[test]
    fn test_transform_hn_items_multiple_items() {
        let items = vec![
            HnItem {
                id: 1,
                item_type: "story".to_string(),
                by: Some("user1".to_string()),
                time: Some(1609459200),
                text: None,
                dead: None,
                deleted: None,
                parent: None,
                kids: None,
                url: Some("https://example1.com".to_string()),
                score: Some(50),
                title: Some("Story 1".to_string()),
                descendants: Some(10),
            },
            HnItem {
                id: 2,
                item_type: "story".to_string(),
                by: Some("user2".to_string()),
                time: Some(1609459300),
                text: None,
                dead: None,
                deleted: None,
                parent: None,
                kids: None,
                url: Some("https://example2.com".to_string()),
                score: Some(75),
                title: Some("Story 2".to_string()),
                descendants: Some(20),
            },
        ];

        let output = transform_hn_items(items, "new".to_string(), 1, 10, 2);

        assert_eq!(output.items.len(), 2);
        assert_eq!(output.items[0].id, 1);
        assert_eq!(output.items[1].id, 2);
    }

    #[test]
    fn test_transform_hn_items_empty() {
        let items: Vec<HnItem> = vec![];
        let output = transform_hn_items(items, "best".to_string(), 1, 10, 0);

        assert_eq!(output.items.len(), 0);
        assert_eq!(output.pagination.total_items, 0);
        assert_eq!(output.pagination.total_pages, 0);
    }

    #[test]
    fn test_transform_hn_items_missing_optional_fields() {
        let items = vec![HnItem {
            id: 999,
            item_type: "story".to_string(),
            by: None,
            time: None,
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: None,
            title: None,
            descendants: None,
        }];

        let output = transform_hn_items(items, "ask".to_string(), 1, 10, 1);

        assert_eq!(output.items[0].id, 999);
        assert_eq!(output.items[0].title, None);
        assert_eq!(output.items[0].author, None);
        assert_eq!(output.items[0].score, None);
        assert_eq!(output.items[0].url, None);
        assert_eq!(output.items[0].time, None);
        assert_eq!(output.items[0].comments, None);
    }

    #[test]
    fn test_transform_hn_items_first_page_no_prev() {
        let items = vec![HnItem {
            id: 1,
            item_type: "story".to_string(),
            by: Some("user".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(10),
            title: Some("Story".to_string()),
            descendants: None,
        }];

        let output = transform_hn_items(items, "top".to_string(), 1, 10, 50);

        assert_eq!(output.pagination.current_page, 1);
        assert!(output.pagination.prev_page_command.is_none());
        assert!(output.pagination.next_page_command.is_some());
        assert_eq!(
            output.pagination.next_page_command.unwrap(),
            "mcptools hn list top --page 2"
        );
    }

    #[test]
    fn test_transform_hn_items_last_page_no_next() {
        let items = vec![HnItem {
            id: 1,
            item_type: "story".to_string(),
            by: Some("user".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(10),
            title: Some("Story".to_string()),
            descendants: None,
        }];

        let output = transform_hn_items(items, "show".to_string(), 5, 10, 50);

        assert_eq!(output.pagination.current_page, 5);
        assert!(output.pagination.next_page_command.is_none());
        assert!(output.pagination.prev_page_command.is_some());
        assert_eq!(
            output.pagination.prev_page_command.unwrap(),
            "mcptools hn list show --page 4"
        );
    }

    #[test]
    fn test_transform_hn_items_middle_page_both_commands() {
        let items = vec![HnItem {
            id: 1,
            item_type: "story".to_string(),
            by: Some("user".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(10),
            title: Some("Story".to_string()),
            descendants: None,
        }];

        let output = transform_hn_items(items, "job".to_string(), 3, 10, 100);

        assert_eq!(output.pagination.current_page, 3);
        assert_eq!(output.pagination.total_pages, 10);
        assert!(output.pagination.next_page_command.is_some());
        assert!(output.pagination.prev_page_command.is_some());
        assert_eq!(
            output.pagination.next_page_command.unwrap(),
            "mcptools hn list job --page 4"
        );
        assert_eq!(
            output.pagination.prev_page_command.unwrap(),
            "mcptools hn list job --page 2"
        );
    }

    #[test]
    fn test_transform_hn_items_different_story_types() {
        let items = vec![HnItem {
            id: 1,
            item_type: "story".to_string(),
            by: Some("user".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(10),
            title: Some("Story".to_string()),
            descendants: None,
        }];

        let story_types = vec!["top", "new", "best", "ask", "show", "job"];

        for story_type in story_types {
            let output = transform_hn_items(items.clone(), story_type.to_string(), 1, 10, 1);
            assert_eq!(output.story_type, story_type);
        }
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <strong>world</strong></p>";
        let stripped = strip_html(html);
        assert_eq!(stripped, "Hello world");
    }

    #[test]
    fn test_strip_html_entities() {
        let html = "1 &lt; 2 &amp; 3 &gt; 0 &quot;test&quot; &#x27;yes&#x27; &#x2F;path";
        let stripped = strip_html(html);
        assert_eq!(stripped, "1 < 2 & 3 > 0 \"test\" 'yes' /path");
    }

    #[test]
    fn test_strip_html_complex() {
        let html = "<p>First paragraph &amp; some <em>emphasis</em></p><p>Second paragraph with &lt;code&gt;</p>";
        let stripped = strip_html(html);
        assert_eq!(
            stripped,
            "First paragraph & some emphasisSecond paragraph with <code>"
        );
    }

    #[test]
    fn test_transform_comments_single() {
        let comments = vec![HnItem {
            id: 100,
            item_type: "comment".to_string(),
            by: Some("commenter".to_string()),
            time: Some(1609459200),
            text: Some("<p>Great post!</p>".to_string()),
            dead: None,
            deleted: None,
            parent: Some(99),
            kids: Some(vec![101, 102]),
            url: None,
            score: None,
            title: None,
            descendants: None,
        }];

        let outputs = transform_comments(comments);

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].id, 100);
        assert_eq!(outputs[0].author, Some("commenter".to_string()));
        assert_eq!(outputs[0].time, Some("2021-01-01 00:00:00 UTC".to_string()));
        assert_eq!(outputs[0].text, Some("Great post!".to_string()));
        assert_eq!(outputs[0].replies_count, 2);
    }

    #[test]
    fn test_transform_comments_multiple() {
        let comments = vec![
            HnItem {
                id: 100,
                item_type: "comment".to_string(),
                by: Some("user1".to_string()),
                time: Some(1609459200),
                text: Some("First comment".to_string()),
                dead: None,
                deleted: None,
                parent: Some(99),
                kids: Some(vec![101]),
                url: None,
                score: None,
                title: None,
                descendants: None,
            },
            HnItem {
                id: 102,
                item_type: "comment".to_string(),
                by: Some("user2".to_string()),
                time: Some(1609459300),
                text: Some("Second comment".to_string()),
                dead: None,
                deleted: None,
                parent: Some(99),
                kids: None,
                url: None,
                score: None,
                title: None,
                descendants: None,
            },
        ];

        let outputs = transform_comments(comments);

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].id, 100);
        assert_eq!(outputs[0].replies_count, 1);
        assert_eq!(outputs[1].id, 102);
        assert_eq!(outputs[1].replies_count, 0);
    }

    #[test]
    fn test_transform_comments_missing_fields() {
        let comments = vec![HnItem {
            id: 100,
            item_type: "comment".to_string(),
            by: None,
            time: None,
            text: None,
            dead: None,
            deleted: None,
            parent: Some(99),
            kids: None,
            url: None,
            score: None,
            title: None,
            descendants: None,
        }];

        let outputs = transform_comments(comments);

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].id, 100);
        assert_eq!(outputs[0].author, None);
        assert_eq!(outputs[0].time, None);
        assert_eq!(outputs[0].text, None);
        assert_eq!(outputs[0].replies_count, 0);
    }

    #[test]
    fn test_build_post_output_full() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: Some("<p>Story text</p>".to_string()),
            dead: None,
            deleted: None,
            parent: None,
            kids: Some(vec![100, 101, 102]),
            url: Some("https://example.com".to_string()),
            score: Some(250),
            title: Some("Test Story".to_string()),
            descendants: Some(50),
        };

        let comments = vec![
            CommentOutput {
                id: 100,
                author: Some("user1".to_string()),
                time: Some("2021-01-01 00:10:00 UTC".to_string()),
                text: Some("First comment".to_string()),
                replies_count: 2,
            },
            CommentOutput {
                id: 101,
                author: Some("user2".to_string()),
                time: Some("2021-01-01 00:20:00 UTC".to_string()),
                text: Some("Second comment".to_string()),
                replies_count: 0,
            },
        ];

        let output = build_post_output(item, comments, 1, 10, 50);

        assert_eq!(output.id, 12345);
        assert_eq!(output.title, Some("Test Story".to_string()));
        assert_eq!(output.url, Some("https://example.com".to_string()));
        assert_eq!(output.author, Some("author".to_string()));
        assert_eq!(output.score, Some(250));
        assert_eq!(output.time, Some("2021-01-01 00:00:00 UTC".to_string()));
        assert_eq!(output.text, Some("Story text".to_string()));
        assert_eq!(output.total_comments, Some(50));
        assert_eq!(output.comments.len(), 2);
        assert_eq!(output.pagination.current_page, 1);
        assert_eq!(output.pagination.total_pages, 5);
        assert_eq!(output.pagination.total_comments, 50);
        assert_eq!(output.pagination.limit, 10);
    }

    #[test]
    fn test_build_post_output_minimal() {
        let item = HnItem {
            id: 999,
            item_type: "story".to_string(),
            by: None,
            time: None,
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: None,
            title: None,
            descendants: None,
        };

        let output = build_post_output(item, vec![], 1, 10, 0);

        assert_eq!(output.id, 999);
        assert_eq!(output.title, None);
        assert_eq!(output.url, None);
        assert_eq!(output.author, None);
        assert_eq!(output.score, None);
        assert_eq!(output.time, None);
        assert_eq!(output.text, None);
        assert_eq!(output.total_comments, None);
        assert_eq!(output.comments.len(), 0);
        assert_eq!(output.pagination.total_comments, 0);
        assert_eq!(output.pagination.total_pages, 0);
    }

    #[test]
    fn test_build_post_output_first_page() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: Some(vec![100, 101]),
            url: None,
            score: Some(100),
            title: Some("Test".to_string()),
            descendants: Some(50),
        };

        let output = build_post_output(item, vec![], 1, 10, 50);

        assert_eq!(output.pagination.current_page, 1);
        assert_eq!(output.pagination.total_pages, 5);
        assert!(output.pagination.prev_page_command.is_none());
        assert!(output.pagination.next_page_command.is_some());
        assert_eq!(
            output.pagination.next_page_command.unwrap(),
            "mcptools hn read 12345 --page 2"
        );
    }

    #[test]
    fn test_build_post_output_last_page() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(100),
            title: Some("Test".to_string()),
            descendants: Some(50),
        };

        let output = build_post_output(item, vec![], 5, 10, 50);

        assert_eq!(output.pagination.current_page, 5);
        assert_eq!(output.pagination.total_pages, 5);
        assert!(output.pagination.next_page_command.is_none());
        assert!(output.pagination.prev_page_command.is_some());
        assert_eq!(
            output.pagination.prev_page_command.unwrap(),
            "mcptools hn read 12345 --page 4"
        );
    }

    #[test]
    fn test_build_post_output_middle_page() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(100),
            title: Some("Test".to_string()),
            descendants: Some(50),
        };

        let output = build_post_output(item, vec![], 3, 10, 50);

        assert_eq!(output.pagination.current_page, 3);
        assert_eq!(output.pagination.total_pages, 5);
        assert!(output.pagination.next_page_command.is_some());
        assert!(output.pagination.prev_page_command.is_some());
        assert_eq!(
            output.pagination.next_page_command.unwrap(),
            "mcptools hn read 12345 --page 4"
        );
        assert_eq!(
            output.pagination.prev_page_command.unwrap(),
            "mcptools hn read 12345 --page 2"
        );
    }

    #[test]
    fn test_build_post_output_single_page() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(100),
            title: Some("Test".to_string()),
            descendants: Some(5),
        };

        let output = build_post_output(item, vec![], 1, 10, 5);

        assert_eq!(output.pagination.current_page, 1);
        assert_eq!(output.pagination.total_pages, 1);
        assert!(output.pagination.next_page_command.is_none());
        assert!(output.pagination.prev_page_command.is_none());
    }

    #[test]
    fn test_build_post_output_empty_comments() {
        let item = HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("author".to_string()),
            time: Some(1609459200),
            text: None,
            dead: None,
            deleted: None,
            parent: None,
            kids: None,
            url: None,
            score: Some(100),
            title: Some("Test".to_string()),
            descendants: Some(0),
        };

        let output = build_post_output(item, vec![], 1, 10, 0);

        assert_eq!(output.comments.len(), 0);
        assert_eq!(output.pagination.total_comments, 0);
        assert_eq!(output.pagination.total_pages, 0);
        assert!(output.pagination.next_page_command.is_none());
        assert!(output.pagination.prev_page_command.is_none());
    }
}
