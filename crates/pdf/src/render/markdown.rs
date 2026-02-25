use crate::render::cleanup::cleanup_text;
use crate::types::ContentBlock;

/// Render a section's content blocks as Markdown.
pub fn render_section_content(blocks: &[ContentBlock]) -> String {
    let mut output = String::new();

    for block in blocks {
        match block {
            ContentBlock::Paragraph(text) => {
                output.push_str(&cleanup_text(text));
                output.push_str("\n\n");
            }
            ContentBlock::Table { headers, rows } => {
                output.push_str(&render_table(headers, rows));
                output.push('\n');
            }
            ContentBlock::Image { id, alt_text } => {
                let alt = alt_text.as_deref().unwrap_or("");
                output.push_str(&format!("![{}](image:{})\n\n", alt, id));
            }
            ContentBlock::SubHeading { level, title } => {
                let hashes = "#".repeat(level.as_u8() as usize);
                output.push_str(&format!("{} {}\n\n", hashes, title));
            }
        }
    }

    output.trim_end().to_string()
}

/// Render a Markdown table from headers and rows.
pub fn render_table(headers: &[String], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push('|');
    for h in headers {
        out.push_str(&format!(" {} |", escape_markdown(h)));
    }
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows {
        out.push('|');
        for (i, cell) in row.iter().enumerate() {
            if i < headers.len() {
                out.push_str(&format!(" {} |", escape_markdown(cell)));
            }
        }
        // Pad missing cells.
        for _ in row.len()..headers.len() {
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

/// Escape Markdown special characters in text.
pub fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HeadingLevel;

    #[test]
    fn test_escape_markdown() {
        assert_eq!(escape_markdown("Hello *world*"), "Hello \\*world\\*");
        assert_eq!(escape_markdown("[link]"), "\\[link\\]");
        assert_eq!(escape_markdown("plain text"), "plain text");
        assert_eq!(escape_markdown("a|b"), "a\\|b");
    }

    #[test]
    fn test_render_table() {
        let headers = vec!["Name".to_string(), "Age".to_string()];
        let rows = vec![vec!["Alice".to_string(), "30".to_string()]];
        let md = render_table(&headers, &rows);
        assert!(md.contains("| Name |"));
        assert!(md.contains("| --- |"));
        assert!(md.contains("| Alice |"));
    }

    #[test]
    fn test_render_table_empty_headers() {
        let md = render_table(&[], &[]);
        assert!(md.is_empty());
    }

    #[test]
    fn test_render_section_content_paragraph() {
        let blocks = vec![ContentBlock::Paragraph("Hello world.".to_string())];
        let md = render_section_content(&blocks);
        assert_eq!(md, "Hello world.");
    }

    #[test]
    fn test_render_section_content_subheading() {
        let blocks = vec![ContentBlock::SubHeading {
            level: HeadingLevel::try_from(2).unwrap(),
            title: "Sub".to_string(),
        }];
        let md = render_section_content(&blocks);
        assert_eq!(md, "## Sub");
    }

    #[test]
    fn test_render_section_content_image() {
        let blocks = vec![ContentBlock::Image {
            id: "img-1".to_string(),
            alt_text: Some("A photo".to_string()),
        }];
        let md = render_section_content(&blocks);
        assert_eq!(md, "![A photo](image:img-1)");
    }

    #[test]
    fn test_render_section_content_mixed() {
        let blocks = vec![
            ContentBlock::SubHeading {
                level: HeadingLevel::try_from(2).unwrap(),
                title: "Details".to_string(),
            },
            ContentBlock::Paragraph("Some text.".to_string()),
            ContentBlock::Table {
                headers: vec!["A".to_string(), "B".to_string()],
                rows: vec![vec!["1".to_string(), "2".to_string()]],
            },
        ];
        let md = render_section_content(&blocks);
        assert!(md.starts_with("## Details"));
        assert!(md.contains("Some text."));
        assert!(md.contains("| A |"));
    }
}
