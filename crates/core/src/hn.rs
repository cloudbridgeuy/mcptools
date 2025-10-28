use chrono::{DateTime, Utc};
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
}
