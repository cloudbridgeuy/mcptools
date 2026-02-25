use std::collections::BTreeMap;

use crate::types::{
    ClassifiedBlock, DocumentMetadata, DocumentTree, HeadingLevel, IndexEntry, Section, SectionId,
    SectionIndex,
};

/// Internal builder that accumulates content before finalizing into a `Section`.
struct SectionBuilder {
    id: SectionId,
    level: u8,
    title: String,
    children: Vec<Section>,
    content_parts: Vec<String>,
    image_count: usize,
    first_page: usize,
    last_page: usize,
}

impl SectionBuilder {
    fn new(id: SectionId, level: u8, title: String, page: usize) -> Self {
        SectionBuilder {
            id,
            level,
            title,
            children: Vec::new(),
            content_parts: Vec::new(),
            image_count: 0,
            first_page: page,
            last_page: page,
        }
    }

    fn append_text(&mut self, text: &str, page: usize) {
        self.content_parts.push(text.to_string());
        self.update_page(page);
    }

    fn append_table(&mut self, headers: &[String], rows: &[Vec<String>], page: usize) {
        let mut parts = Vec::new();
        if !headers.is_empty() {
            parts.push(headers.join(" | "));
        }
        for row in rows {
            parts.push(row.join(" | "));
        }
        if !parts.is_empty() {
            self.content_parts.push(parts.join("\n"));
        }
        self.update_page(page);
    }

    fn append_image(&mut self, page: usize) {
        self.image_count += 1;
        self.update_page(page);
    }

    fn add_child(&mut self, child: Section) {
        self.last_page = self.last_page.max(child.page_range.1);
        self.children.push(child);
    }

    fn update_page(&mut self, page: usize) {
        self.last_page = self.last_page.max(page);
    }

    fn build(self) -> Section {
        let full_text = self.content_parts.join(" ");
        let preview = content_preview(&full_text, 100);
        Section {
            id: self.id,
            level: HeadingLevel::try_from(self.level).unwrap_or(HeadingLevel::H1),
            title: self.title,
            children: self.children,
            content_preview: preview,
            image_count: self.image_count,
            page_range: (self.first_page, self.last_page),
        }
    }
}

/// Build a document tree from classified blocks using a stack-based nesting algorithm.
///
/// Headings create nested sections based on their level. Content blocks (Paragraph, Table, Image)
/// are attached to the most recent heading's section. If no headings are found, one section per
/// page is created as a fallback.
pub fn build_tree(blocks: &[ClassifiedBlock], metadata: DocumentMetadata) -> DocumentTree {
    let has_headings = blocks
        .iter()
        .any(|b| matches!(b, ClassifiedBlock::Heading { .. }));

    let sections = if has_headings {
        build_sections_from_headings(blocks)
    } else {
        build_fallback_sections(blocks, &metadata)
    };

    let index = build_section_index(&sections);
    let title = metadata
        .title
        .clone()
        .or_else(|| sections.first().map(|s| s.title.clone()))
        .unwrap_or_else(|| "Untitled".to_string());

    DocumentTree {
        title,
        metadata,
        sections,
        index,
    }
}

/// Stack-based nesting algorithm for heading-driven section building.
fn build_sections_from_headings(blocks: &[ClassifiedBlock]) -> Vec<Section> {
    // Per-level counters for generating SectionId(depth, index).
    let mut level_counters: [usize; 7] = [0; 7]; // index 0 unused, 1..=6

    // Stack of (SectionBuilder, heading_level).
    let mut stack: Vec<(SectionBuilder, u8)> = Vec::new();
    // Finished top-level sections collected here.
    let mut root_sections: Vec<Section> = Vec::new();

    for block in blocks {
        match block {
            ClassifiedBlock::Heading { level, title, page } => {
                let lvl = (*level).clamp(1, 6);

                // Pop sections from the stack that have level >= this heading's level.
                // Each popped section becomes a child of the section beneath it, or a root section.
                while let Some((_top_builder, top_level)) = stack.last() {
                    if *top_level >= lvl {
                        let (builder, _) = stack.pop().unwrap();
                        let finished = builder.build();
                        if let Some((parent, _)) = stack.last_mut() {
                            parent.add_child(finished);
                        } else {
                            root_sections.push(finished);
                        }
                    } else {
                        break;
                    }
                }

                let idx = level_counters[lvl as usize];
                level_counters[lvl as usize] += 1;
                let id = SectionId::new(lvl, idx);
                let builder = SectionBuilder::new(id, lvl, title.clone(), *page);
                stack.push((builder, lvl));
            }
            ClassifiedBlock::Paragraph { text, page } => {
                if let Some((builder, _)) = stack.last_mut() {
                    builder.append_text(text, *page);
                }
                // If no heading has been seen yet, content is discarded (no section to attach to).
            }
            ClassifiedBlock::Table {
                headers,
                rows,
                page,
            } => {
                if let Some((builder, _)) = stack.last_mut() {
                    builder.append_table(headers, rows, *page);
                }
            }
            ClassifiedBlock::Image { id: _, page } => {
                if let Some((builder, _)) = stack.last_mut() {
                    builder.append_image(*page);
                }
            }
        }
    }

    // Unwind the entire stack.
    while let Some((builder, _)) = stack.pop() {
        let finished = builder.build();
        if let Some((parent, _)) = stack.last_mut() {
            parent.add_child(finished);
        } else {
            root_sections.push(finished);
        }
    }

    root_sections
}

/// Fallback: create one section per page when no headings are found.
fn build_fallback_sections(
    blocks: &[ClassifiedBlock],
    metadata: &DocumentMetadata,
) -> Vec<Section> {
    // Group blocks by page.
    let mut page_contents: BTreeMap<usize, (Vec<String>, usize)> = BTreeMap::new();

    for block in blocks {
        let (page, text, is_image) = match block {
            ClassifiedBlock::Paragraph { text, page } => (*page, Some(text.clone()), false),
            ClassifiedBlock::Table {
                headers,
                rows,
                page,
            } => {
                let mut parts = Vec::new();
                if !headers.is_empty() {
                    parts.push(headers.join(" | "));
                }
                for row in rows {
                    parts.push(row.join(" | "));
                }
                (*page, Some(parts.join("\n")), false)
            }
            ClassifiedBlock::Image { page, .. } => (*page, None, true),
            ClassifiedBlock::Heading { page, .. } => {
                // Shouldn't happen in fallback path, but handle gracefully.
                (*page, None, false)
            }
        };

        let entry = page_contents.entry(page).or_insert_with(|| (Vec::new(), 0));
        if let Some(t) = text {
            entry.0.push(t);
        }
        if is_image {
            entry.1 += 1;
        }
    }

    // If there are no blocks at all but we know the page count, create empty sections.
    if page_contents.is_empty() && metadata.page_count > 0 {
        return (0..metadata.page_count)
            .map(|p| {
                let page_num = p + 1;
                Section {
                    id: SectionId::new(1, p),
                    level: HeadingLevel::H1,
                    title: format!("Page {}", page_num),
                    children: Vec::new(),
                    content_preview: String::new(),
                    image_count: 0,
                    page_range: (page_num, page_num),
                }
            })
            .collect();
    }

    page_contents
        .into_iter()
        .enumerate()
        .map(|(idx, (page, (texts, img_count)))| {
            let full_text = texts.join(" ");
            let preview = content_preview(&full_text, 100);
            Section {
                id: SectionId::new(1, idx),
                level: HeadingLevel::H1,
                title: format!("Page {}", page),
                children: Vec::new(),
                content_preview: preview,
                image_count: img_count,
                page_range: (page, page),
            }
        })
        .collect()
}

/// Build a flat index of all sections with breadcrumb paths.
pub fn build_section_index(sections: &[Section]) -> SectionIndex {
    let mut entries = Vec::new();
    for section in sections {
        flatten_section(section, &[], &mut entries);
    }
    SectionIndex { entries }
}

/// Recursively flatten a section tree into index entries, accumulating breadcrumb paths.
fn flatten_section(section: &Section, parent_path: &[String], entries: &mut Vec<IndexEntry>) {
    let mut path = parent_path.to_vec();
    path.push(section.title.clone());

    entries.push(IndexEntry {
        id: section.id.clone(),
        level: section.level,
        title: section.title.clone(),
        path: path.clone(),
        image_count: section.image_count,
    });

    for child in &section.children {
        flatten_section(child, &path, entries);
    }
}

/// Produce a content preview truncated to approximately `max_len` characters on a word boundary.
///
/// If the text fits within `max_len`, it is returned as-is. Otherwise, the text is truncated
/// at the last space character before `max_len` and an ellipsis is appended.
fn content_preview(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }

    // Find a char-boundary-safe truncation point.
    let boundary = trimmed.floor_char_boundary(max_len);
    let search_region = &trimmed[..boundary];
    if let Some(last_space) = search_region.rfind(' ') {
        format!("{}...", &trimmed[..last_space])
    } else {
        // No space found; hard-truncate at the char boundary.
        format!("{}...", search_region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClassifiedBlock, DocumentMetadata};

    fn default_metadata() -> DocumentMetadata {
        DocumentMetadata {
            title: Some("Test Document".to_string()),
            author: None,
            page_count: 1,
            creator: None,
        }
    }

    // --- content_preview tests ---

    #[test]
    fn test_content_preview_short_text() {
        assert_eq!(content_preview("hello world", 100), "hello world");
    }

    #[test]
    fn test_content_preview_exact_fit() {
        let text = "a".repeat(100);
        assert_eq!(content_preview(&text, 100), text);
    }

    #[test]
    fn test_content_preview_word_boundary() {
        let text = "the quick brown fox jumps over the lazy dog and more words follow here to exceed the limit easily now";
        let preview = content_preview(text, 50);
        assert!(preview.ends_with("..."));
        // The part before "..." should end at a word boundary.
        let without_ellipsis = preview.trim_end_matches("...");
        assert!(
            without_ellipsis.len() <= 50,
            "Preview body '{}' is {} chars, expected <= 50",
            without_ellipsis,
            without_ellipsis.len()
        );
        // Should not have a trailing space.
        assert!(!without_ellipsis.ends_with(' '));
    }

    #[test]
    fn test_content_preview_no_space() {
        let text = "a".repeat(150);
        let preview = content_preview(&text, 100);
        assert_eq!(preview, format!("{}...", "a".repeat(100)));
    }

    #[test]
    fn test_content_preview_empty() {
        assert_eq!(content_preview("", 100), "");
    }

    #[test]
    fn test_content_preview_whitespace_trimmed() {
        assert_eq!(content_preview("  hello  ", 100), "hello");
    }

    #[test]
    fn test_content_preview_multibyte_chars() {
        // CJK characters are 3 bytes each in UTF-8.
        // "日本語テスト" = 6 chars, 18 bytes. Truncating at byte 10 would
        // split a character boundary — must not panic.
        let text = "日本語テスト is Japanese";
        let preview = content_preview(text, 10);
        assert!(preview.ends_with("..."));
        // Should not panic and should contain valid UTF-8.
        assert!(preview.is_char_boundary(0));
    }

    // --- build_tree: empty input ---

    #[test]
    fn test_empty_input() {
        let meta = DocumentMetadata {
            title: Some("Empty".to_string()),
            author: None,
            page_count: 0,
            creator: None,
        };
        let tree = build_tree(&[], meta);
        assert_eq!(tree.title, "Empty");
        assert!(tree.sections.is_empty());
        assert!(tree.index.entries.is_empty());
    }

    // --- build_tree: simple heading hierarchy H1 > H2 > H2 ---

    #[test]
    fn test_simple_heading_hierarchy() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Intro paragraph".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Section 1.1".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Content of 1.1".to_string(),
                page: 2,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Section 1.2".to_string(),
                page: 3,
            },
            ClassifiedBlock::Paragraph {
                text: "Content of 1.2".to_string(),
                page: 3,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());

        // Should have one top-level section (H1).
        assert_eq!(tree.sections.len(), 1);
        let chapter = &tree.sections[0];
        assert_eq!(chapter.title, "Chapter 1");
        assert_eq!(chapter.level.as_u8(), 1);
        assert_eq!(chapter.content_preview, "Intro paragraph");

        // H1 should have two H2 children.
        assert_eq!(chapter.children.len(), 2);
        assert_eq!(chapter.children[0].title, "Section 1.1");
        assert_eq!(chapter.children[0].level.as_u8(), 2);
        assert_eq!(chapter.children[0].content_preview, "Content of 1.1");
        assert_eq!(chapter.children[1].title, "Section 1.2");
        assert_eq!(chapter.children[1].level.as_u8(), 2);
        assert_eq!(chapter.children[1].content_preview, "Content of 1.2");
    }

    // --- build_tree: nested H1 > H2 > H3 produces children correctly ---

    #[test]
    fn test_nested_h1_h2_h3() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "H1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "H2".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "H2 content".to_string(),
                page: 2,
            },
            ClassifiedBlock::Heading {
                level: 3,
                title: "H3".to_string(),
                page: 3,
            },
            ClassifiedBlock::Paragraph {
                text: "H3 content".to_string(),
                page: 3,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());

        // One root H1.
        assert_eq!(tree.sections.len(), 1);
        let h1 = &tree.sections[0];
        assert_eq!(h1.title, "H1");
        assert_eq!(h1.level.as_u8(), 1);

        // H1 has one H2 child.
        assert_eq!(h1.children.len(), 1);
        let h2 = &h1.children[0];
        assert_eq!(h2.title, "H2");
        assert_eq!(h2.level.as_u8(), 2);
        assert_eq!(h2.content_preview, "H2 content");

        // H2 has one H3 child.
        assert_eq!(h2.children.len(), 1);
        let h3 = &h2.children[0];
        assert_eq!(h3.title, "H3");
        assert_eq!(h3.level.as_u8(), 3);
        assert_eq!(h3.content_preview, "H3 content");
    }

    // --- build_tree: fallback when no headings ---

    #[test]
    fn test_fallback_no_headings() {
        let blocks = vec![
            ClassifiedBlock::Paragraph {
                text: "Page one text".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "More page one text".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img1".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Page two text".to_string(),
                page: 2,
            },
        ];

        let meta = DocumentMetadata {
            title: None,
            author: None,
            page_count: 2,
            creator: None,
        };
        let tree = build_tree(&blocks, meta);

        // Should get one section per page (pages 1 and 2).
        assert_eq!(tree.sections.len(), 2);
        assert_eq!(tree.sections[0].title, "Page 1");
        assert_eq!(
            tree.sections[0].content_preview,
            "Page one text More page one text"
        );
        assert_eq!(tree.sections[0].image_count, 0);

        assert_eq!(tree.sections[1].title, "Page 2");
        assert_eq!(tree.sections[1].image_count, 1);

        // Title should come from first section since metadata title is None.
        assert_eq!(tree.title, "Page 1");
    }

    // --- build_section_index: breadcrumb paths ---

    #[test]
    fn test_breadcrumb_paths_in_index() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Root".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Child".to_string(),
                page: 2,
            },
            ClassifiedBlock::Heading {
                level: 3,
                title: "Grandchild".to_string(),
                page: 3,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());

        assert_eq!(tree.index.entries.len(), 3);

        // Root: path = ["Root"]
        assert_eq!(tree.index.entries[0].title, "Root");
        assert_eq!(tree.index.entries[0].path, vec!["Root"]);

        // Child: path = ["Root", "Child"]
        assert_eq!(tree.index.entries[1].title, "Child");
        assert_eq!(tree.index.entries[1].path, vec!["Root", "Child"]);

        // Grandchild: path = ["Root", "Child", "Grandchild"]
        assert_eq!(tree.index.entries[2].title, "Grandchild");
        assert_eq!(
            tree.index.entries[2].path,
            vec!["Root", "Child", "Grandchild"]
        );
    }

    // --- build_tree: image counting ---

    #[test]
    fn test_image_count_tracking() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Intro".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img2".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Details".to_string(),
                page: 2,
            },
            ClassifiedBlock::Image {
                id: "img3".to_string(),
                page: 2,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());
        let h1 = &tree.sections[0];
        assert_eq!(h1.image_count, 2);
        assert_eq!(h1.children[0].image_count, 1);
    }

    // --- build_tree: page range tracking ---

    #[test]
    fn test_page_range_tracking() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Start".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Sub".to_string(),
                page: 3,
            },
            ClassifiedBlock::Paragraph {
                text: "End".to_string(),
                page: 5,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());
        let chapter = &tree.sections[0];
        // Chapter starts at page 1, and its child extends to page 5.
        assert_eq!(chapter.page_range, (1, 5));
        assert_eq!(chapter.children[0].page_range, (3, 5));
    }

    // --- build_tree: table content in preview ---

    #[test]
    fn test_table_content_in_preview() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Data".to_string(),
                page: 1,
            },
            ClassifiedBlock::Table {
                headers: vec!["Name".to_string(), "Value".to_string()],
                rows: vec![vec!["A".to_string(), "1".to_string()]],
                page: 1,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());
        let section = &tree.sections[0];
        // Preview should include table text.
        assert!(section.content_preview.contains("Name"));
        assert!(section.content_preview.contains("Value"));
    }

    // --- build_tree: multiple H1 siblings ---

    #[test]
    fn test_multiple_h1_siblings() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "First".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Content A".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Second".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Content B".to_string(),
                page: 2,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());
        assert_eq!(tree.sections.len(), 2);
        assert_eq!(tree.sections[0].title, "First");
        assert_eq!(tree.sections[1].title, "Second");
    }

    // --- SectionId generation ---

    #[test]
    fn test_section_id_generation() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "A".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "B".to_string(),
                page: 2,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "C".to_string(),
                page: 3,
            },
        ];

        let tree = build_tree(&blocks, default_metadata());
        assert_eq!(tree.sections[0].id.as_str(), "s-1-0");
        assert_eq!(tree.sections[0].children[0].id.as_str(), "s-2-0");
        assert_eq!(tree.sections[0].children[1].id.as_str(), "s-2-1");
    }
}
