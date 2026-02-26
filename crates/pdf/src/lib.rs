use std::collections::{BTreeMap, HashMap};

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

        blocks.sort_by_key(|b| b.page());

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

    /// List images in a section or the whole document with section and page context.
    pub fn list_section_images(
        &self,
        id: Option<&SectionId>,
    ) -> Result<Vec<EnrichedImageRef>, PdfError> {
        if let Some(section_id) = id {
            if !self.tree.index.entries.iter().any(|e| &e.id == section_id) {
                return Err(PdfError::SectionNotFound(section_id.to_string()));
            }
        }

        let locations = collect_image_locations(&self.blocks, id);
        let format_map = self.build_image_format_map();
        Ok(build_enriched_image_refs(locations, &format_map))
    }

    /// Build a map of image ID to format by scanning all pages.
    fn build_image_format_map(&self) -> HashMap<String, ImageFormat> {
        self.backend
            .pages()
            .values()
            .filter_map(|&page_id| images::list_page_images(&self.backend, page_id).ok())
            .flatten()
            .map(|img_ref| (img_ref.id.as_str().to_string(), img_ref.format))
            .collect()
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

/// List images in a section or the whole document with section and page context.
pub fn list_section_images(
    bytes: &[u8],
    id: Option<&SectionId>,
) -> Result<Vec<EnrichedImageRef>, PdfError> {
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

/// Location of an image within the document's section structure.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageLocation {
    id: String,
    page: usize,
    section_id: SectionId,
    section_title: String,
}

/// Walk classified blocks and collect `ImageLocation`s with section and page context.
/// Uses the same stack algorithm as `collect_section_content`.
/// Pre-heading images are assigned to `s-0-0` / "(Document)".
/// When `target_id` is `None`, all images are collected (whole-document mode).
fn collect_image_locations(
    blocks: &[ClassifiedBlock],
    target_id: Option<&SectionId>,
) -> Vec<ImageLocation> {
    let mut level_counters: [usize; 7] = [0; 7];
    let mut stack: Vec<(SectionId, u8, String)> = Vec::new();
    let mut locations: Vec<ImageLocation> = Vec::new();

    let collect_all = target_id.is_none();

    let doc_section_id = SectionId::new(0, 0);
    let doc_section_title = "(Document)".to_string();

    let ancestor_is_target = |stack: &[(SectionId, u8, String)]| {
        collect_all || stack.iter().any(|(id, _, _)| Some(id) == target_id)
    };

    for block in blocks {
        match block {
            ClassifiedBlock::Heading { level, title, .. } => {
                let lvl = (*level).clamp(1, 6);

                while let Some((_, top_level, _)) = stack.last() {
                    if *top_level >= lvl {
                        stack.pop();
                    } else {
                        break;
                    }
                }

                let idx = level_counters[lvl as usize];
                level_counters[lvl as usize] += 1;
                let id = SectionId::new(lvl, idx);

                stack.push((id, lvl, title.clone()));
            }
            ClassifiedBlock::Image { id, page } => {
                if ancestor_is_target(&stack) {
                    let (section_id, section_title) = match stack.last() {
                        Some((sid, _, title)) => (sid.clone(), title.clone()),
                        None => (doc_section_id.clone(), doc_section_title.clone()),
                    };
                    locations.push(ImageLocation {
                        id: id.clone(),
                        page: *page,
                        section_id,
                        section_title,
                    });
                }
            }
            _ => {}
        }
    }

    locations
}

/// Combine image locations with a format lookup map to produce enriched image refs.
/// Images whose ID is not found in the format map are filtered out.
fn build_enriched_image_refs(
    locations: Vec<ImageLocation>,
    format_map: &HashMap<String, ImageFormat>,
) -> Vec<EnrichedImageRef> {
    locations
        .into_iter()
        .filter_map(|loc| {
            let format = format_map.get(&loc.id)?;
            Some(EnrichedImageRef {
                id: ImageId::new(loc.id),
                format: *format,
                section_id: loc.section_id,
                section_title: loc.section_title,
                page: loc.page,
            })
        })
        .collect()
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

                if ancestor_is_target(&stack) {
                    content_blocks.push(ContentBlock::SubHeading {
                        level: HeadingLevel::try_from(lvl).unwrap_or(HeadingLevel::H1),
                        title: title.clone(),
                    });
                }

                stack.push((id, lvl));
            }
            ClassifiedBlock::Paragraph { text, .. } => {
                if ancestor_is_target(&stack) {
                    content_blocks.push(ContentBlock::Paragraph(text.clone()));
                }
            }
            ClassifiedBlock::Table { headers, rows, .. } => {
                if ancestor_is_target(&stack) {
                    content_blocks.push(ContentBlock::Table {
                        headers: headers.clone(),
                        rows: rows.clone(),
                    });
                }
            }
            ClassifiedBlock::Image { id, .. } => {
                if ancestor_is_target(&stack) {
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

        assert_eq!(content.len(), 3); // paragraph + subheading + child paragraph
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Root content"));
        assert!(matches!(&content[1], ContentBlock::SubHeading { title, .. } if title == "Child"));
        assert!(matches!(&content[2], ContentBlock::Paragraph(t) if t == "Child content"));
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

    // -- collect_image_locations tests --

    #[test]
    fn test_collect_image_locations_empty() {
        let locations = collect_image_locations(&[], Some(&SectionId::new(1, 0)));
        assert!(locations.is_empty());
    }

    #[test]
    fn test_collect_image_locations_whole_doc() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
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
            ClassifiedBlock::Image {
                id: "img2".to_string(),
                page: 3,
            },
        ];

        let locations = collect_image_locations(&blocks, None);
        assert_eq!(locations.len(), 2);

        assert_eq!(locations[0].id, "img1");
        assert_eq!(locations[0].page, 1);
        assert_eq!(locations[0].section_id, SectionId::new(1, 0));
        assert_eq!(locations[0].section_title, "Chapter 1");

        assert_eq!(locations[1].id, "img2");
        assert_eq!(locations[1].page, 3);
        assert_eq!(locations[1].section_id, SectionId::new(1, 1));
        assert_eq!(locations[1].section_title, "Chapter 2");
    }

    #[test]
    fn test_collect_image_locations_targeted_section() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
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
            ClassifiedBlock::Image {
                id: "img2".to_string(),
                page: 3,
            },
        ];

        let target = SectionId::new(1, 0);
        let locations = collect_image_locations(&blocks, Some(&target));
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, "img1");
        assert_eq!(locations[0].section_id, SectionId::new(1, 0));
    }

    #[test]
    fn test_collect_image_locations_pre_heading_images() {
        let blocks = vec![
            ClassifiedBlock::Image {
                id: "pre_img".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "ch1_img".to_string(),
                page: 2,
            },
        ];

        let locations = collect_image_locations(&blocks, None);
        assert_eq!(locations.len(), 2);

        assert_eq!(locations[0].id, "pre_img");
        assert_eq!(locations[0].section_id, SectionId::new(0, 0));
        assert_eq!(locations[0].section_title, "(Document)");

        assert_eq!(locations[1].id, "ch1_img");
        assert_eq!(locations[1].section_id, SectionId::new(1, 0));
    }

    #[test]
    fn test_collect_image_locations_nested_headings() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Section".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "nested_img".to_string(),
                page: 2,
            },
        ];

        let locations = collect_image_locations(&blocks, None);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].section_id, SectionId::new(2, 0));
        assert_eq!(locations[0].section_title, "Section");
    }

    #[test]
    fn test_collect_image_locations_sibling_headings() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 2,
                title: "Section A".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "imgA".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Section B".to_string(),
                page: 2,
            },
            ClassifiedBlock::Image {
                id: "imgB".to_string(),
                page: 2,
            },
        ];

        let locations = collect_image_locations(&blocks, None);
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].section_id, SectionId::new(2, 0));
        assert_eq!(locations[0].section_title, "Section A");
        assert_eq!(locations[1].section_id, SectionId::new(2, 1));
        assert_eq!(locations[1].section_title, "Section B");
    }

    // -- build_enriched_image_refs tests --

    #[test]
    fn test_build_enriched_image_refs_basic() {
        let locations = vec![ImageLocation {
            id: "img1".to_string(),
            page: 3,
            section_id: SectionId::new(1, 0),
            section_title: "Chapter 1".to_string(),
        }];
        let mut format_map = HashMap::new();
        format_map.insert("img1".to_string(), ImageFormat::Jpeg);

        let refs = build_enriched_image_refs(locations, &format_map);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id.as_str(), "img1");
        assert_eq!(refs[0].format, ImageFormat::Jpeg);
        assert_eq!(refs[0].section_id, SectionId::new(1, 0));
        assert_eq!(refs[0].section_title, "Chapter 1");
        assert_eq!(refs[0].page, 3);
    }

    #[test]
    fn test_build_enriched_image_refs_missing_format_filtered() {
        let locations = vec![
            ImageLocation {
                id: "img1".to_string(),
                page: 1,
                section_id: SectionId::new(1, 0),
                section_title: "Ch1".to_string(),
            },
            ImageLocation {
                id: "img_missing".to_string(),
                page: 2,
                section_id: SectionId::new(1, 1),
                section_title: "Ch2".to_string(),
            },
        ];
        let mut format_map = HashMap::new();
        format_map.insert("img1".to_string(), ImageFormat::Png);

        let refs = build_enriched_image_refs(locations, &format_map);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id.as_str(), "img1");
    }

    #[test]
    fn test_build_enriched_image_refs_empty() {
        let format_map = HashMap::new();
        let refs = build_enriched_image_refs(vec![], &format_map);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_stable_sort_interleaves_images_by_page() {
        let mut blocks = [
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Intro".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Chapter 2".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Body".to_string(),
                page: 2,
            },
            // Images appended late (simulating the bug)
            ClassifiedBlock::Image {
                id: "img1".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "img2".to_string(),
                page: 2,
            },
        ];

        blocks.sort_by_key(ClassifiedBlock::page);

        // Page 1 blocks first, page 2 blocks second
        assert_eq!(blocks[0].page(), 1);
        assert_eq!(blocks[1].page(), 1);
        assert_eq!(blocks[2].page(), 1);
        assert_eq!(blocks[3].page(), 2);
        assert_eq!(blocks[4].page(), 2);
        assert_eq!(blocks[5].page(), 2);

        // Within page 1: Heading, Paragraph, Image (stable order)
        assert!(
            matches!(&blocks[0], ClassifiedBlock::Heading { title, .. } if title == "Chapter 1")
        );
        assert!(matches!(&blocks[1], ClassifiedBlock::Paragraph { text, .. } if text == "Intro"));
        assert!(matches!(&blocks[2], ClassifiedBlock::Image { id, .. } if id == "img1"));

        // Within page 2: Heading, Paragraph, Image
        assert!(
            matches!(&blocks[3], ClassifiedBlock::Heading { title, .. } if title == "Chapter 2")
        );
        assert!(matches!(&blocks[4], ClassifiedBlock::Paragraph { text, .. } if text == "Body"));
        assert!(matches!(&blocks[5], ClassifiedBlock::Image { id, .. } if id == "img2"));
    }

    #[test]
    fn test_collect_section_content_hierarchical_inclusion() {
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
            ClassifiedBlock::Image {
                id: "child_img".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let (content, images) = collect_section_content(&blocks, Some(&target));

        // Root paragraph + Child subheading + Child paragraph + Child image = 4
        assert_eq!(content.len(), 4);
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Root content"));
        assert!(matches!(&content[1], ContentBlock::SubHeading { title, .. } if title == "Child"));
        assert!(matches!(&content[2], ContentBlock::Paragraph(t) if t == "Child content"));
        assert!(matches!(&content[3], ContentBlock::Image { id, .. } if id == "child_img"));
        assert_eq!(images, vec!["child_img"]);
    }

    #[test]
    fn test_collect_section_content_sibling_excluded() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Target".to_string(),
                page: 1,
            },
            ClassifiedBlock::Paragraph {
                text: "Target content".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Sibling".to_string(),
                page: 2,
            },
            ClassifiedBlock::Paragraph {
                text: "Sibling content".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let (content, _) = collect_section_content(&blocks, Some(&target));

        // Only "Target content" paragraph — sibling's content excluded
        assert_eq!(content.len(), 1);
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Target content"));
    }

    #[test]
    fn test_collect_image_locations_hierarchical() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Parent".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Child".to_string(),
                page: 2,
            },
            ClassifiedBlock::Image {
                id: "child_img".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let locations = collect_image_locations(&blocks, Some(&target));

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, "child_img");
        // Image's section_id comes from immediate parent (H2), not the target (H1)
        assert_eq!(locations[0].section_id, SectionId::new(2, 0));
        assert_eq!(locations[0].section_title, "Child");
    }

    #[test]
    fn test_collect_image_locations_sibling_excluded() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Target".to_string(),
                page: 1,
            },
            ClassifiedBlock::Image {
                id: "target_img".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 1,
                title: "Sibling".to_string(),
                page: 2,
            },
            ClassifiedBlock::Image {
                id: "sibling_img".to_string(),
                page: 2,
            },
        ];

        let target = SectionId::new(1, 0);
        let locations = collect_image_locations(&blocks, Some(&target));

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, "target_img");
    }

    #[test]
    fn test_collect_image_locations_deeply_nested() {
        let blocks = vec![
            ClassifiedBlock::Heading {
                level: 1,
                title: "Top".to_string(),
                page: 1,
            },
            ClassifiedBlock::Heading {
                level: 2,
                title: "Mid".to_string(),
                page: 2,
            },
            ClassifiedBlock::Heading {
                level: 3,
                title: "Leaf".to_string(),
                page: 3,
            },
            ClassifiedBlock::Image {
                id: "deep_img".to_string(),
                page: 3,
            },
        ];

        let target = SectionId::new(1, 0);
        let locations = collect_image_locations(&blocks, Some(&target));

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, "deep_img");
        assert_eq!(locations[0].section_id, SectionId::new(3, 0));
        assert_eq!(locations[0].section_title, "Leaf");
    }
}
