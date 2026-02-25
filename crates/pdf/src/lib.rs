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

    /// Read a specific section's content rendered as Markdown.
    pub fn read_section(&self, id: &SectionId) -> Result<SectionContent, PdfError> {
        // Verify the section exists in the index.
        let entry = self
            .tree
            .index
            .entries
            .iter()
            .find(|e| &e.id == id)
            .ok_or_else(|| PdfError::SectionNotFound(id.to_string()))?;

        let title = entry.title.clone();

        // Walk classified blocks with the same stack logic to find content
        // belonging to this section.
        let (content_blocks, image_ids) = collect_section_content(&self.blocks, id);

        // Render content blocks as Markdown.
        let text = render::markdown::render_section_content(&content_blocks);

        // Build image references.
        let pages = self.backend.pages();
        let images = build_image_refs(&self.backend, &pages, &image_ids);

        Ok(SectionContent {
            id: id.clone(),
            title,
            text,
            images,
        })
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

/// Read a specific section's content as Markdown.
pub fn read_section(bytes: &[u8], id: &SectionId) -> Result<SectionContent, PdfError> {
    ParsedDocument::from_bytes(bytes)?.read_section(id)
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

/// Walk classified blocks using the same stack algorithm as `build_tree` and
/// collect the `ContentBlock`s and image IDs that belong to the target section.
fn collect_section_content(
    blocks: &[ClassifiedBlock],
    target_id: &SectionId,
) -> (Vec<ContentBlock>, Vec<String>) {
    let mut level_counters: [usize; 7] = [0; 7];
    let mut stack: Vec<(SectionId, u8)> = Vec::new();
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut image_ids: Vec<String> = Vec::new();

    // Content belongs to whichever section sits at the top of the stack.
    let top_is_target =
        |stack: &[(SectionId, u8)]| stack.last().is_some_and(|(id, _)| id == target_id);

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

                // If this heading is a child of the target section, add it
                // as a sub-heading in the content.
                if stack.iter().any(|(sid, _)| sid == target_id) {
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
        let (blocks, images) = collect_section_content(&[], &SectionId::new(1, 0));
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
        let (content, images) = collect_section_content(&blocks, &target);

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

        // Reading the root section should get only root's direct content,
        // plus the child heading as a SubHeading.
        let target = SectionId::new(1, 0);
        let (content, _) = collect_section_content(&blocks, &target);

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

        // Reading the child section should get child's content.
        let target = SectionId::new(2, 0);
        let (content, _) = collect_section_content(&blocks, &target);

        assert_eq!(content.len(), 1);
        assert!(matches!(&content[0], ContentBlock::Paragraph(t) if t == "Child content"));
    }

    #[test]
    fn test_extract_metadata() {
        // This test just verifies the function signature and basic behavior.
        // Full integration testing requires a real PDF.
        let result = info(&[]);
        assert!(result.is_err()); // Empty bytes can't be parsed
    }
}
