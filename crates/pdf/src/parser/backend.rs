use std::collections::BTreeMap;

use lopdf::{self, content::Content};

use crate::PdfError;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// A page identifier mirroring `lopdf::ObjectId`: (object number, generation number).
pub type PageId = (u32, u16);

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Font information extracted from a page's resource dictionary.
#[derive(Debug, Clone)]
pub struct BackendFontInfo {
    /// The font name key as it appears in the resource dictionary (e.g. `b"F1"`).
    pub name: Vec<u8>,
    /// Base font name from the font dictionary, if present.
    pub base_font: Option<String>,
    /// Font subtype (e.g. `Type1`, `TrueType`, `Type0`).
    pub subtype: Option<String>,
    /// Encoding entry from the font dictionary, if present.
    pub encoding: Option<String>,
}

/// A simplified, lopdf-independent representation of a PDF value.
///
/// This enum decouples higher-level logic from the concrete `lopdf::Object`
/// type so that the functional core can work with pure data.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfValue {
    Null,
    Bool(bool),
    Integer(i64),
    Real(f32),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Array(Vec<PdfValue>),
    Dict(Vec<(Vec<u8>, PdfValue)>),
    Reference(PageId),
}

/// A single content-stream operation (operator + operands).
#[derive(Debug, Clone)]
pub struct ContentOp {
    pub operator: String,
    pub operands: Vec<PdfValue>,
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Extract an `f32` from a [`PdfValue`], accepting both `Integer` and `Real`.
pub fn get_number_from_value(val: &PdfValue) -> Option<f32> {
    match val {
        PdfValue::Integer(i) => Some(*i as f32),
        PdfValue::Real(f) => Some(*f),
        _ => None,
    }
}

/// Convert a `lopdf::Object` into a [`PdfValue`].
///
/// References are preserved as `PdfValue::Reference`.  Stream dictionaries
/// are converted but the raw stream bytes are discarded (they must be
/// obtained through [`PdfBackend::page_content`]).
pub fn convert_object(obj: &lopdf::Object) -> PdfValue {
    match obj {
        lopdf::Object::Null => PdfValue::Null,
        lopdf::Object::Boolean(b) => PdfValue::Bool(*b),
        lopdf::Object::Integer(i) => PdfValue::Integer(*i),
        lopdf::Object::Real(f) => PdfValue::Real(*f),
        lopdf::Object::Name(n) => PdfValue::Name(n.clone()),
        lopdf::Object::String(s, _) => PdfValue::Str(s.clone()),
        lopdf::Object::Array(arr) => PdfValue::Array(arr.iter().map(convert_object).collect()),
        lopdf::Object::Dictionary(dict) => {
            let entries = dict
                .iter()
                .map(|(k, v)| (k.clone(), convert_object(v)))
                .collect();
            PdfValue::Dict(entries)
        }
        lopdf::Object::Stream(stream) => {
            let entries = stream
                .dict
                .iter()
                .map(|(k, v)| (k.clone(), convert_object(v)))
                .collect();
            PdfValue::Dict(entries)
        }
        lopdf::Object::Reference(id) => PdfValue::Reference(*id),
    }
}

/// Best-effort decoding of raw PDF string bytes into a Rust `String`.
///
/// Handles three cases in order:
/// 1. UTF-16BE with BOM (`\xFE\xFF` prefix) -- strips BOM and decodes.
/// 2. Valid UTF-8 -- returned as-is.
/// 3. Fallback to Latin-1 (ISO 8859-1) -- each byte mapped to its Unicode
///    code point.
pub fn decode_text_simple(bytes: &[u8]) -> String {
    // UTF-16BE with BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let payload = &bytes[2..];
        let code_units: Vec<u16> = payload
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(u16::from_be_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect();
        return String::from_utf16_lossy(&code_units);
    }

    // Try UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // Fallback: Latin-1 (PDFDocEncoding for the printable range).
    bytes.iter().map(|&b| b as char).collect()
}

// ---------------------------------------------------------------------------
// PdfBackend trait
// ---------------------------------------------------------------------------

/// Abstraction over a PDF parsing backend (currently backed by `lopdf`).
///
/// This trait exists so that higher-level modules in the functional core can
/// be tested against mock implementations without pulling in the full lopdf
/// dependency.
pub trait PdfBackend {
    /// Return a mapping from 1-based page number to [`PageId`].
    fn pages(&self) -> BTreeMap<u32, PageId>;

    /// Return font information for every font referenced by the given page.
    fn page_fonts(&self, page: PageId) -> Result<Vec<BackendFontInfo>, PdfError>;

    /// Return the raw (possibly compressed) content stream bytes for a page.
    fn page_content(&self, page: PageId) -> Result<Vec<u8>, PdfError>;

    /// Decode raw content-stream bytes into a sequence of [`ContentOp`]s.
    fn decode_content(&self, data: &[u8]) -> Result<Vec<ContentOp>, PdfError>;

    /// Decode raw string bytes found in a text-showing operator, using any
    /// font-specific encoding information the backend can find for the given
    /// page and font name.
    fn decode_text(&self, page: PageId, font_name: &[u8], bytes: &[u8]) -> String;
}

// ---------------------------------------------------------------------------
// LopdfBackend
// ---------------------------------------------------------------------------

/// Concrete [`PdfBackend`] implementation backed by [`lopdf::Document`].
pub struct LopdfBackend {
    doc: lopdf::Document,
}

impl LopdfBackend {
    /// Parse a PDF from an in-memory byte slice.
    pub fn load_bytes(data: &[u8]) -> Result<Self, PdfError> {
        let doc = lopdf::Document::load_mem(data).map_err(|e| PdfError::Parse(e.to_string()))?;

        if doc.is_encrypted() {
            return Err(PdfError::Encrypted);
        }

        Ok(Self { doc })
    }

    /// Direct access to the underlying `lopdf::Document`.
    pub fn raw_doc(&self) -> &lopdf::Document {
        &self.doc
    }

    /// Total number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.doc.get_pages().len()
    }

    /// Extract metadata from the PDF trailer's Info dictionary.
    ///
    /// Returns a `BTreeMap` of keys such as `"Title"`, `"Author"`,
    /// `"Creator"`, `"Producer"`, `"Subject"`, `"Keywords"`,
    /// `"CreationDate"`, and `"ModDate"`.
    pub fn metadata(&self) -> BTreeMap<String, String> {
        let mut meta = BTreeMap::new();

        let info_ref = match self.doc.trailer.get(b"Info") {
            Ok(obj) => obj,
            Err(_) => return meta,
        };

        let info_dict = match info_ref {
            lopdf::Object::Reference(id) => match self.doc.get_object(*id) {
                Ok(lopdf::Object::Dictionary(d)) => d,
                _ => return meta,
            },
            lopdf::Object::Dictionary(d) => d,
            _ => return meta,
        };

        let keys: &[&[u8]] = &[
            b"Title",
            b"Author",
            b"Creator",
            b"Producer",
            b"Subject",
            b"Keywords",
            b"CreationDate",
            b"ModDate",
        ];

        for key in keys {
            if let Ok(obj) = info_dict.get(key) {
                let value = match obj {
                    lopdf::Object::String(bytes, _) => decode_text_simple(bytes),
                    lopdf::Object::Name(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                    _ => continue,
                };
                let key_str = String::from_utf8_lossy(key).into_owned();
                meta.insert(key_str, value);
            }
        }

        meta
    }

    /// Extract page dimensions `(width, height)` from the MediaBox.
    ///
    /// MediaBox is an array `[llx, lly, urx, ury]`.  Dimensions are
    /// computed as `(urx - llx, ury - lly)`.
    pub fn page_dimensions(&self, page: PageId) -> Result<(f32, f32), PdfError> {
        let page_obj = self
            .doc
            .get_object(page)
            .map_err(|e| PdfError::Parse(format!("cannot get page object: {}", e)))?;

        let page_dict = page_obj
            .as_dict()
            .map_err(|e| PdfError::Parse(format!("page object is not a dictionary: {}", e)))?;

        let media_box = self
            .find_media_box(page_dict)
            .ok_or_else(|| PdfError::Parse("MediaBox not found for page".into()))?;

        let nums = self.array_to_f32s(&media_box)?;
        if nums.len() < 4 {
            return Err(PdfError::Parse(format!(
                "MediaBox has {} elements, expected 4",
                nums.len()
            )));
        }

        let width = nums[2] - nums[0];
        let height = nums[3] - nums[1];
        Ok((width, height))
    }

    // -- private helpers ----------------------------------------------------

    /// Walk up the page tree to find the MediaBox array.
    fn find_media_box(&self, dict: &lopdf::Dictionary) -> Option<Vec<lopdf::Object>> {
        if let Ok(obj) = dict.get(b"MediaBox") {
            if let Some(arr) = self.resolve_array(obj) {
                return Some(arr);
            }
        }

        // Recurse into Parent.
        if let Ok(parent_ref) = dict.get(b"Parent") {
            if let Ok(parent_id) = parent_ref.as_reference() {
                if let Ok(parent_obj) = self.doc.get_object(parent_id) {
                    if let Ok(parent_dict) = parent_obj.as_dict() {
                        return self.find_media_box(parent_dict);
                    }
                }
            }
        }

        None
    }

    /// Resolve an object to an array, following a single level of indirection.
    fn resolve_array(&self, obj: &lopdf::Object) -> Option<Vec<lopdf::Object>> {
        match obj {
            lopdf::Object::Array(arr) => Some(arr.clone()),
            lopdf::Object::Reference(id) => {
                if let Ok(resolved) = self.doc.get_object(*id) {
                    if let Ok(arr) = resolved.as_array() {
                        return Some(arr.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Convert a vector of lopdf objects to `f32` values.
    fn array_to_f32s(&self, objects: &[lopdf::Object]) -> Result<Vec<f32>, PdfError> {
        objects
            .iter()
            .map(|obj| {
                let resolved = match obj {
                    lopdf::Object::Reference(id) => self
                        .doc
                        .get_object(*id)
                        .map_err(|e| PdfError::Parse(e.to_string()))?,
                    other => other,
                };
                match resolved {
                    lopdf::Object::Integer(i) => Ok(*i as f32),
                    lopdf::Object::Real(f) => Ok(*f),
                    _ => Err(PdfError::Parse(format!(
                        "expected number in array, got {:?}",
                        resolved
                    ))),
                }
            })
            .collect()
    }

    /// Look up the encoding name for a font on a page.
    ///
    /// Returns the encoding name (e.g. `"WinAnsiEncoding"`,
    /// `"MacRomanEncoding"`) if declared in the font dictionary, or `None`
    /// if no encoding entry exists or the font cannot be found.
    fn font_encoding_name(&self, page: PageId, font_name: &[u8]) -> Option<String> {
        let fonts = self.doc.get_page_fonts(page).ok()?;
        let font_dict = fonts.get(font_name)?;
        let enc_obj = font_dict.get(b"Encoding").ok()?;
        match enc_obj {
            lopdf::Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// PdfBackend implementation for LopdfBackend
// ---------------------------------------------------------------------------

impl PdfBackend for LopdfBackend {
    fn pages(&self) -> BTreeMap<u32, PageId> {
        self.doc.get_pages()
    }

    fn page_fonts(&self, page: PageId) -> Result<Vec<BackendFontInfo>, PdfError> {
        let fonts_map = self
            .doc
            .get_page_fonts(page)
            .map_err(|e| PdfError::Parse(format!("cannot get page fonts: {}", e)))?;

        let mut result = Vec::with_capacity(fonts_map.len());
        for (name, dict) in &fonts_map {
            let base_font = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned());

            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned());

            let encoding = dict.get(b"Encoding").ok().and_then(|o| match o {
                lopdf::Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
                _ => None,
            });

            result.push(BackendFontInfo {
                name: name.clone(),
                base_font,
                subtype,
                encoding,
            });
        }

        Ok(result)
    }

    fn page_content(&self, page: PageId) -> Result<Vec<u8>, PdfError> {
        self.doc
            .get_page_content(page)
            .map_err(|e| PdfError::Parse(format!("cannot get page content: {}", e)))
    }

    fn decode_content(&self, data: &[u8]) -> Result<Vec<ContentOp>, PdfError> {
        let content = Content::decode(data)
            .map_err(|e| PdfError::Parse(format!("content stream decode error: {}", e)))?;

        let ops = content
            .operations
            .into_iter()
            .map(|op| ContentOp {
                operator: op.operator,
                operands: op.operands.iter().map(convert_object).collect(),
            })
            .collect();

        Ok(ops)
    }

    fn decode_text(&self, page: PageId, font_name: &[u8], bytes: &[u8]) -> String {
        // Check the font's declared encoding for hints.
        if let Some(enc_name) = self.font_encoding_name(page, font_name) {
            // Identity-H / Identity-V fonts typically use 2-byte CID codes
            // that map to Unicode.  Try UTF-16BE decoding.
            if enc_name.contains("Identity") && bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
                let code_units: Vec<u16> = bytes
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                let decoded = String::from_utf16_lossy(&code_units);
                if !decoded.is_empty() && !decoded.chars().all(|c| c == '\u{FFFD}' || c == '\0') {
                    return decoded;
                }
            }
        }

        // Fallback to generic heuristic.
        decode_text_simple(bytes)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- decode_text_simple -------------------------------------------------

    #[test]
    fn decode_text_simple_utf8() {
        let input = "Hello, world!";
        assert_eq!(decode_text_simple(input.as_bytes()), "Hello, world!");
    }

    #[test]
    fn decode_text_simple_utf8_multibyte() {
        // "cafe" followed by U+00E9 -- valid UTF-8 multi-byte.
        let input = "caf\u{00E9}";
        assert_eq!(decode_text_simple(input.as_bytes()), "caf\u{00E9}");
    }

    #[test]
    fn decode_text_simple_latin1() {
        // 0xE9 is U+00E9 in Latin-1 but not valid standalone UTF-8.
        let input: &[u8] = &[0x63, 0x61, 0x66, 0xE9];
        let result = decode_text_simple(input);
        assert_eq!(result, "caf\u{00E9}");
    }

    #[test]
    fn decode_text_simple_latin1_high_bytes() {
        // 0xA9 = copyright, 0xAE = registered sign
        let input: &[u8] = &[0xA9, 0x20, 0xAE];
        let result = decode_text_simple(input);
        assert_eq!(result, "\u{00A9} \u{00AE}");
    }

    #[test]
    fn decode_text_simple_utf16be_basic() {
        // UTF-16BE BOM followed by "AB"
        let input: &[u8] = &[0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        assert_eq!(decode_text_simple(input), "AB");
    }

    #[test]
    fn decode_text_simple_utf16be_non_ascii() {
        // UTF-16BE BOM followed by U+00E9
        let input: &[u8] = &[0xFE, 0xFF, 0x00, 0xE9];
        assert_eq!(decode_text_simple(input), "\u{00E9}");
    }

    #[test]
    fn decode_text_simple_utf16be_empty_payload() {
        let input: &[u8] = &[0xFE, 0xFF];
        assert_eq!(decode_text_simple(input), "");
    }

    #[test]
    fn decode_text_simple_utf16be_odd_trailing_byte() {
        // Trailing odd byte should be silently ignored.
        let input: &[u8] = &[0xFE, 0xFF, 0x00, 0x41, 0x00];
        assert_eq!(decode_text_simple(input), "A");
    }

    #[test]
    fn decode_text_simple_empty() {
        assert_eq!(decode_text_simple(&[]), "");
    }

    // -- get_number_from_value ----------------------------------------------

    #[test]
    fn get_number_integer() {
        assert_eq!(get_number_from_value(&PdfValue::Integer(42)), Some(42.0));
    }

    #[test]
    fn get_number_real() {
        assert_eq!(get_number_from_value(&PdfValue::Real(2.72)), Some(2.72));
    }

    #[test]
    fn get_number_negative() {
        assert_eq!(get_number_from_value(&PdfValue::Integer(-10)), Some(-10.0));
    }

    #[test]
    fn get_number_from_non_numeric() {
        assert_eq!(get_number_from_value(&PdfValue::Null), None);
        assert_eq!(get_number_from_value(&PdfValue::Bool(true)), None);
        assert_eq!(
            get_number_from_value(&PdfValue::Name(b"Foo".to_vec())),
            None
        );
        assert_eq!(
            get_number_from_value(&PdfValue::Str(b"text".to_vec())),
            None
        );
        assert_eq!(get_number_from_value(&PdfValue::Array(vec![])), None);
        assert_eq!(get_number_from_value(&PdfValue::Dict(vec![])), None);
        assert_eq!(get_number_from_value(&PdfValue::Reference((1, 0))), None);
    }

    // -- convert_object -----------------------------------------------------

    #[test]
    fn convert_null() {
        assert_eq!(convert_object(&lopdf::Object::Null), PdfValue::Null);
    }

    #[test]
    fn convert_boolean() {
        assert_eq!(
            convert_object(&lopdf::Object::Boolean(true)),
            PdfValue::Bool(true),
        );
        assert_eq!(
            convert_object(&lopdf::Object::Boolean(false)),
            PdfValue::Bool(false),
        );
    }

    #[test]
    fn convert_integer() {
        assert_eq!(
            convert_object(&lopdf::Object::Integer(99)),
            PdfValue::Integer(99),
        );
    }

    #[test]
    fn convert_real() {
        assert_eq!(
            convert_object(&lopdf::Object::Real(1.5)),
            PdfValue::Real(1.5),
        );
    }

    #[test]
    fn convert_name() {
        assert_eq!(
            convert_object(&lopdf::Object::Name(b"Font".to_vec())),
            PdfValue::Name(b"Font".to_vec()),
        );
    }

    #[test]
    fn convert_string_literal() {
        assert_eq!(
            convert_object(&lopdf::Object::String(
                b"hello".to_vec(),
                lopdf::StringFormat::Literal,
            )),
            PdfValue::Str(b"hello".to_vec()),
        );
    }

    #[test]
    fn convert_string_hex() {
        assert_eq!(
            convert_object(&lopdf::Object::String(
                b"AABB".to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )),
            PdfValue::Str(b"AABB".to_vec()),
        );
    }

    #[test]
    fn convert_array() {
        let arr = lopdf::Object::Array(vec![lopdf::Object::Integer(1), lopdf::Object::Real(2.0)]);
        assert_eq!(
            convert_object(&arr),
            PdfValue::Array(vec![PdfValue::Integer(1), PdfValue::Real(2.0)]),
        );
    }

    #[test]
    fn convert_empty_array() {
        let arr = lopdf::Object::Array(vec![]);
        assert_eq!(convert_object(&arr), PdfValue::Array(vec![]));
    }

    #[test]
    fn convert_dictionary() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Key", lopdf::Object::Boolean(true));
        let obj = lopdf::Object::Dictionary(dict);

        match convert_object(&obj) {
            PdfValue::Dict(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].0, b"Key");
                assert_eq!(entries[0].1, PdfValue::Bool(true));
            }
            other => panic!("expected Dict, got {:?}", other),
        }
    }

    #[test]
    fn convert_stream_uses_dict() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Length", lopdf::Object::Integer(0));
        let stream = lopdf::Stream::new(dict, vec![]);
        let obj = lopdf::Object::Stream(stream);

        match convert_object(&obj) {
            PdfValue::Dict(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].0, b"Length");
                assert_eq!(entries[0].1, PdfValue::Integer(0));
            }
            other => panic!("expected Dict for stream, got {:?}", other),
        }
    }

    #[test]
    fn convert_reference() {
        let obj = lopdf::Object::Reference((7, 0));
        assert_eq!(convert_object(&obj), PdfValue::Reference((7, 0)));
    }

    #[test]
    fn convert_nested_array_in_dict() {
        let mut dict = lopdf::Dictionary::new();
        dict.set(
            "Box",
            lopdf::Object::Array(vec![
                lopdf::Object::Integer(0),
                lopdf::Object::Integer(0),
                lopdf::Object::Real(612.0),
                lopdf::Object::Real(792.0),
            ]),
        );
        let obj = lopdf::Object::Dictionary(dict);

        match convert_object(&obj) {
            PdfValue::Dict(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0].1,
                    PdfValue::Array(vec![
                        PdfValue::Integer(0),
                        PdfValue::Integer(0),
                        PdfValue::Real(612.0),
                        PdfValue::Real(792.0),
                    ]),
                );
            }
            other => panic!("expected Dict, got {:?}", other),
        }
    }
}
