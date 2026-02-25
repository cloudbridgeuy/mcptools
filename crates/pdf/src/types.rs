use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionId(String);

impl SectionId {
    pub fn new(depth: u8, index: usize) -> Self {
        SectionId(format!("s-{}-{}", depth, index))
    }

    pub fn parse(s: &str) -> Result<Self, InvalidSectionId> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 || parts[0] != "s" {
            return Err(InvalidSectionId);
        }
        parts[1].parse::<u8>().map_err(|_| InvalidSectionId)?;
        parts[2].parse::<usize>().map_err(|_| InvalidSectionId)?;
        Ok(SectionId(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    /// Heading level 1 -- useful as a default fallback when clamping.
    pub const H1: Self = HeadingLevel(1);

    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for HeadingLevel {
    type Error = InvalidHeadingLevel;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (1..=6).contains(&value) {
            Ok(HeadingLevel(value))
        } else {
            Err(InvalidHeadingLevel)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageId(String);

impl ImageId {
    pub fn new(id: impl Into<String>) -> Self {
        ImageId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ImageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Jpeg2000,
    Gif,
    Tiff,
    Bmp,
    WebP,
    Unknown,
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageFormat::Jpeg => write!(f, "jpeg"),
            ImageFormat::Png => write!(f, "png"),
            ImageFormat::Jpeg2000 => write!(f, "jpeg2000"),
            ImageFormat::Gif => write!(f, "gif"),
            ImageFormat::Tiff => write!(f, "tiff"),
            ImageFormat::Bmp => write!(f, "bmp"),
            ImageFormat::WebP => write!(f, "webp"),
            ImageFormat::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentTree {
    pub title: String,
    pub metadata: DocumentMetadata,
    pub sections: Vec<Section>,
    pub index: SectionIndex,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Section {
    pub id: SectionId,
    pub level: HeadingLevel,
    pub title: String,
    pub children: Vec<Section>,
    pub content_preview: String,
    pub image_count: usize,
    pub page_range: (usize, usize),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SectionIndex {
    pub entries: Vec<IndexEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IndexEntry {
    pub id: SectionId,
    pub level: HeadingLevel,
    pub title: String,
    pub path: Vec<String>,
    pub image_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SectionContent {
    pub id: SectionId,
    pub title: String,
    pub text: String,
    pub images: Vec<ImageRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageRef {
    pub id: ImageId,
    pub format: ImageFormat,
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub id: ImageId,
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: usize,
    pub creator: Option<String>,
}

/// Content block types for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Paragraph(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Image {
        id: String,
        alt_text: Option<String>,
    },
    SubHeading {
        level: HeadingLevel,
        title: String,
    },
}

/// Classified block output from the table detection pipeline.
#[derive(Debug, Clone)]
pub enum ClassifiedBlock {
    Heading {
        level: u8,
        title: String,
        page: usize,
    },
    Paragraph {
        text: String,
        page: usize,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        page: usize,
    },
    Image {
        id: String,
        page: usize,
    },
}

#[derive(Debug, Error)]
#[error("Heading level must be between 1 and 6")]
pub struct InvalidHeadingLevel;

#[derive(Debug, Error)]
#[error("Invalid section ID format (expected 's-{{depth}}-{{index}}')")]
pub struct InvalidSectionId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_level_valid() {
        assert!(HeadingLevel::try_from(1).is_ok());
        assert!(HeadingLevel::try_from(6).is_ok());
    }

    #[test]
    fn test_heading_level_invalid() {
        assert!(HeadingLevel::try_from(0).is_err());
        assert!(HeadingLevel::try_from(7).is_err());
    }

    #[test]
    fn test_heading_level_accessor() {
        let h = HeadingLevel::try_from(3).unwrap();
        assert_eq!(h.as_u8(), 3);
    }

    #[test]
    fn test_section_id_new() {
        let id = SectionId::new(1, 0);
        assert_eq!(id.as_str(), "s-1-0");
    }

    #[test]
    fn test_section_id_parse_valid() {
        assert!(SectionId::parse("s-1-0").is_ok());
        assert!(SectionId::parse("s-3-42").is_ok());
    }

    #[test]
    fn test_section_id_parse_invalid() {
        assert!(SectionId::parse("invalid").is_err());
        assert!(SectionId::parse("s-0").is_err());
        assert!(SectionId::parse("x-1-0").is_err());
    }

    #[test]
    fn test_image_format_display() {
        assert_eq!(format!("{}", ImageFormat::Jpeg), "jpeg");
        assert_eq!(format!("{}", ImageFormat::Unknown), "unknown");
    }
}
