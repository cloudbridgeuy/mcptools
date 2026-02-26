use std::collections::BTreeMap;

use thiserror::Error;

use parser::backend::PdfBackend;

pub mod images;
pub mod parser;
pub mod render;
pub mod tree;
pub mod types;

pub use types::*;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("PDF parsing error: {0}")]
    Parse(String),
    #[error("Document is encrypted")]
    Encrypted,
    #[error("Section not found: {0}")]
    SectionNotFound(String),
    #[error("Image not found: {0}")]
    ImageNotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<InvalidSectionId> for PdfError {
    fn from(e: InvalidSectionId) -> Self {
        PdfError::Parse(e.to_string())
    }
}

impl From<InvalidHeadingLevel> for PdfError {
    fn from(e: InvalidHeadingLevel) -> Self {
        PdfError::Parse(e.to_string())
    }
}

impl From<InvalidPeekPosition> for PdfError {
    fn from(e: InvalidPeekPosition) -> Self {
        PdfError::Parse(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A parsed PDF document holding all intermediate state.
///
/// Constructed via [`ParsedDocument::from_bytes`]. Provides methods for
/// reading the document tree, section content, images, and metadata without
/// re-parsing from bytes.
pub struct ParsedDocument {
    backend: parser::backend::LopdfBackend,
    blocks: Vec<ClassifiedBlock>,
    pub tree: DocumentTree,
}

impl ParsedDocument {
    /// Parse PDF bytes into a navigable document.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PdfError> {
        let backend = parser::backend::LopdfBackend::load_bytes(bytes)?;
        let all_spans = parser::layout::extract_all_pages(&backend)?;
        let page_blocks = parser::layout::analyze(all_spans)?;
        let mut blocks = parser::table::classify_blocks(page_blocks);

        // Inject image blocks by scanning each page's XObject resources.
        let page_map = backend.pages();
        for (&page_num, &page_id) in &page_map {
            if let Ok(page_images) = images::list_page_images(&backend, page_id) {
                for img_ref in page_images {
                    blocks.push(ClassifiedBlock::Image {
                        id: img_ref.id.to_string(),
                        page: page_num as usize,
                    });
                }
            }
        }

        let metadata = extract_metadata(&backend);
        let tree = tree::build_tree(&blocks, metadata);

        Ok(ParsedDocument {
            backend,
            blocks,
            tree,
        })
    }

    /// Read a section's content rendered as Markdown.
    /// When `id` is `None`, reads the entire document.
    pub fn read_section(&self, id: Option<&SectionId>) -> Result<SectionContent, PdfError> {
        let (title, section_id) = match id {
            Some(id) => {
                let entry = self
                    .tree
                    .index
                    .entries
                    .iter()
                    .find(|e| &e.id == id)
                    .ok_or_else(|| PdfError::SectionNotFound(id.to_string()))?;
                (entry.title.clone(), id.clone())
            }
            None => (self.tree.title.clone(), SectionId::new(0, 0)),
        };

        let (content_blocks, image_ids) = collect_section_content(&self.blocks, id);
        let text = render::markdown::render_section_content(&content_blocks);
        let pages = self.backend.pages();
        let images = build_image_refs(&self.backend, &pages, &image_ids);

        Ok(SectionContent {
            id: section_id,
            title,
            text,
            images,
        })
    }

    /// Peek at a section's text content with a window.
    ///
    /// Computes the character offset from `position` and `limit` automatically.
    pub fn peek_section(
        &self,
        id: Option<&SectionId>,
        position: PeekPosition,
        limit: usize,
    ) -> Result<PeekContent, PdfError> {
        let section = self.read_section(id)?;
        let total_chars = section.text.chars().count();
        let offset = compute_peek_offset(position, total_chars, limit);
        let snippet = extract_window(&section.text, offset, limit).to_string();

        Ok(PeekContent {
            id: id.cloned(),
            title: section.title,
            snippet,
            position,
            total_chars,
        })
    }

    /// List images in a section or the whole document.
    pub fn list_section_images(&self, id: Option<&SectionId>) -> Result<Vec<ImageRef>, PdfError> {
        if let Some(section_id) = id {
            if !self.tree.index.entries.iter().any(|e| &e.id == section_id) {
                return Err(PdfError::SectionNotFound(section_id.to_string()));
            }
        }

        let (_content_blocks, image_ids) = collect_section_content(&self.blocks, id);
        let pages = self.backend.pages();
        Ok(build_image_refs(&self.backend, &pages, &image_ids))
    }

    /// Extract a specific image by its ID.
    pub fn get_image(&self, id: &ImageId) -> Result<ImageData, PdfError> {
        images::extract_image(&self.backend, id)
    }

    /// Get document metadata.
    pub fn metadata(&self) -> &DocumentMetadata {
        &self.tree.metadata
    }
}

// ---------------------------------------------------------------------------
// Convenience free functions (stateless, re-parse each call)
// ---------------------------------------------------------------------------

/// Parse PDF bytes into a document tree.
pub fn parse(bytes: &[u8]) -> Result<DocumentTree, PdfError> {
    Ok(ParsedDocument::from_bytes(bytes)?.tree)
}

/// Read a section's content as Markdown, or the whole document if `id` is `None`.
pub fn read_section(bytes: &[u8], id: Option<&SectionId>) -> Result<SectionContent, PdfError> {
    ParsedDocument::from_bytes(bytes)?.read_section(id)
}

/// Peek at a section's text content.
pub fn peek_section(
    bytes: &[u8],
    id: Option<&SectionId>,
    position: PeekPosition,
    limit: usize,
) -> Result<PeekContent, PdfError> {
    ParsedDocument::from_bytes(bytes)?.peek_section(id, position, limit)
}

/// List images in a section or the whole document.
pub fn list_section_images(
    bytes: &[u8],
    id: Option<&SectionId>,
) -> Result<Vec<ImageRef>, PdfError> {
    ParsedDocument::from_bytes(bytes)?.list_section_images(id)
}

/// Extract a specific image by ID.
pub fn get_image(bytes: &[u8], id: &ImageId) -> Result<ImageData, PdfError> {
    let backend = parser::backend::LopdfBackend::load_bytes(bytes)?;
    images::extract_image(&backend, id)
}

/// Get document metadata without building the full tree.
pub fn info(bytes: &[u8]) -> Result<DocumentMetadata, PdfError> {
    let backend = parser::backend::LopdfBackend::load_bytes(bytes)?;
    Ok(extract_metadata(&backend))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_metadata(backend: &parser::backend::LopdfBackend) -> DocumentMetadata {
    let raw = backend.metadata();
    DocumentMetadata {
        title: raw.get("Title").cloned(),
        author: raw.get("Author").cloned(),
        page_count: backend.page_count(),
        creator: raw.get("Creator").cloned(),
    }
}

/// Compute the character offset for a given peek position, total character count, and limit.
fn compute_peek_offset(position: PeekPosition, total_chars: usize, limit: usize) -> usize {
    match position {
        PeekPosition::Beginning => 0,
        PeekPosition::Middle => total_chars.saturating_sub(limit) / 2,
        PeekPosition::Ending => total_chars.saturating_sub(limit),
        PeekPosition::Random => {
            if total_chars <= limit {
                0
            } else {
                use rand::Rng;
                rand::thread_rng().gen_range(0..=total_chars.saturating_sub(limit))
            }
        }
    }
}

/// Extract a window of text at the given character offset and limit.
///
/// Operates on character boundaries (not byte boundaries) to handle Unicode safely.
/// Returns an empty string if offset is beyond the text length.
pub fn extract_window(text: &str, offset: usize, limit: usize) -> &str {
    if limit == 0 || text.is_empty() {
        return "";
    }

    let char_count = text.chars().count();
    if offset >= char_count {
        return "";
    }

    let end_char = (offset + limit).min(char_count);

    // Map char indices to byte indices.
    let mut chars = text.char_indices();
    let start_byte = chars.nth(offset).map(|(i, _)| i).unwrap_or(text.len());

    // Advance to the char at index `end_char` (exclusive boundary).
    // After nth(offset), the iterator is positioned at offset+1. We need to
    // reach end_char, so advance by (end_char - offset - 1) more positions.
    let end_byte = chars
        .nth(end_char - offset - 1)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    &text[start_byte..end_byte]
}

/// Walk classified blocks using the same stack algorithm as `build_tree` and
/// collect the `ContentBlock`s and image IDs that belong to the target section.
/// When `target_id` is `None`, all content is collected (whole-document mode).
fn collect_section_content(
    blocks: &[ClassifiedBlock],
    target_id: Option<&SectionId>,
) -> (Vec<ContentBlock>, Vec<String>) {
    let mut level_counters: [usize; 7] = [0; 7];
    let mut stack: Vec<(SectionId, u8)> = Vec::new();
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut image_ids: Vec<String> = Vec::new();

    let collect_all = target_id.is_none();

    // Content belongs to whichever section sits at the top of the stack.
    let top_is_target = |stack: &[(SectionId, u8)]| {
        collect_all || stack.last().is_some_and(|(id, _)| Some(id) == target_id)
    };

    // Check whether any ancestor in the stack matches the target.
    let ancestor_is_target = |stack: &[(SectionId, u8)]| {
        collect_all || stack.iter().any(|(sid, _)| Some(sid) == target_id)
    };

    for block in blocks {
        match block {
            ClassifiedBlock::Heading { level, title, .. } => {
                let lvl = (*level).clamp(1, 6);

                // Pop entries with level >= this heading.
                while let Some((_, top_level)) = stack.last() {
                    if *top_level >= lvl {
                        stack.pop();
                    } else {
                        break;
                    }
                }

                let idx = level_counters[lvl as usize];
                level_counters[lvl as usize] += 1;
                let id = SectionId::new(lvl, idx);

                if collect_all || ancestor_is_target(&stack) {
                    content_blocks.push(ContentBlock::SubHeading {
                        level: HeadingLevel::try_from(lvl).unwrap_or(HeadingLevel::H1),
                        title: title.clone(),
                    });
                }

                stack.push((id, lvl));
            }
            ClassifiedBlock::Paragraph { text, .. } => {
                if top_is_target(&stack) {
                    content_blocks.push(ContentBlock::Paragraph(text.clone()));
                }
            }
            ClassifiedBlock::Table { headers, rows, .. } => {
                if top_is_target(&stack) {
                    content_blocks.push(ContentBlock::Table {
                        headers: headers.clone(),
                        rows: rows.clone(),
                    });
                }
            }
            ClassifiedBlock::Image { id, .. } => {
                if top_is_target(&stack) {
                    image_ids.push(id.clone());
                    content_blocks.push(ContentBlock::Image {
                        id: id.clone(),
                        alt_text: None,
                    });
                }
            }
        }
    }

    (content_blocks, image_ids)
}

/// Build `ImageRef`s for a set of image IDs by querying page resources.
fn build_image_refs(
    backend: &parser::backend::LopdfBackend,
    pages: &BTreeMap<u32, parser::backend::PageId>,
    image_ids: &[String],
) -> Vec<ImageRef> {
    if image_ids.is_empty() {
        return Vec::new();
    }

    let mut refs = Vec::new();
    for &page_id in pages.values() {
        if let Ok(page_images) = images::list_page_images(backend, page_id) {
            refs.extend(
                page_images
                    .into_iter()
                    .filter(|img| image_ids.iter().any(|id| id == img.id.as_str())),
            );
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_section_content_empty() {
        let (blocks, images) = collect_section_content(&[], Some(&SectionId::new(1, 0)));
        assert!(blocks.is_empty());
        assert!(images.is_empty());
    }

    #[test]
    fn test_collect_section_content_target_section() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Hello world".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 2".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Not in chapter 1".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let (content, images) = collect_section_content(&blocks, Some(&target));

        assert_eq!(content.len(), 2); // paragraph + image
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Hello world"));
        assert!(matches!(&content[1], ContentBlock::Image { id, .. } if id == "img1"));
        assert_eq!(images, vec!["img1"]);
    }

    #[test]
    fn test_collect_section_content_with_child_subheadings() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Root".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Root content".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Child".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Child content".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let (content, _) = collect_section_content(&blocks, Some(&target));

        assert_eq!(content.len(), 2); // paragraph + subheading
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Root content"));
        assert!(matches!(&content[1], ContentBlock::SubHeading { title, .. } if title == "Child"));
    }

    #[test]
    fn test_collect_section_content_child_section() {
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
            ClassifiedBlock::Paragraph {
                text: "Child content".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(2, 0);
        let (content, _) = collect_section_content(&blocks, Some(&target));

        assert_eq!(content.len(), 1);
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Child content"));
    }

    #[test]
    fn test_collect_section_content_none_empty() {
        let (blocks, images) = collect_section_content(&[], None);
        assert!(blocks.is_empty());
        assert!(images.is_empty());
    }

    #[test]
    fn test_collect_section_content_none_collects_all() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Ch 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Para 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Ch 2".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Para 2".to_string(),
                page: 2,
            },
            ClassifiedBlock::Table {
                headers: vec!["A".to_string()],
                rows: vec![vec!["1".to_string()]],
                page: 2,
            },
        ];

        let (content, images) = collect_section_content(&blocks, None);

        // 2 SubHeadings + 2 Paragraphs + 1 Image + 1 Table = 6
        assert_eq!(content.len(), 6);
        assert_eq!(images, vec!["img1"]);
    }

    #[test]
    fn test_extract_window_beginning() {
        assert_eq!(extract_window("Hello, world!", 0, 5), "Hello");
    }

    #[test]
    fn test_extract_window_middle() {
        assert_eq!(extract_window("Hello, world!", 7, 5), "world");
    }

    #[test]
    fn test_extract_window_ending() {
        assert_eq!(extract_window("The end.", 4, 10), "end.");
    }

    #[test]
    fn test_extract_window_shorter_text() {
        assert_eq!(extract_window("Hi", 0, 100), "Hi");
    }

    #[test]
    fn test_extract_window_empty_text() {
        assert_eq!(extract_window("", 0, 10), "");
    }

    #[test]
    fn test_extract_window_exact_limit() {
        assert_eq!(extract_window("Exact", 0, 5), "Exact");
    }

    #[test]
    fn test_extract_window_offset_beyond_length() {
        assert_eq!(extract_window("Beyond", 20, 10), "");
    }

    #[test]
    fn test_extract_window_limit_zero() {
        assert_eq!(extract_window("Hello", 0, 0), "");
    }

    #[test]
    fn test_extract_window_single_char_at_end() {
        assert_eq!(extract_window("Hello!", 5, 1), "!");
    }

    #[test]
    fn test_extract_window_unicode() {
        let text = "😊😀😃😁😆";
        assert_eq!(extract_window(text, 0, 3), "😊😀😃");
        assert_eq!(extract_window(text, 2, 2), "😃😁");
        assert_eq!(extract_window(text, 4, 10), "😆");
    }

    #[test]
    fn test_extract_metadata() {
        let result = info(&[]);
        assert!(result.is_err());
    }
}
