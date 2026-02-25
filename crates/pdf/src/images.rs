use crate::parser::backend::LopdfBackend;
use crate::types::{ImageData, ImageFormat, ImageId, ImageRef};
use crate::PdfError;

/// Detect the image format from raw bytes using magic byte signatures.
///
/// Returns `ImageFormat::Unknown` if the bytes are too short (< 8) or no
/// known signature matches.
pub fn detect_image_format(bytes: &[u8]) -> ImageFormat {
    if bytes.len() < 8 {
        return ImageFormat::Unknown;
    }

    // JPEG: FF D8 FF
    if bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return ImageFormat::Jpeg;
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return ImageFormat::Png;
    }

    // JPEG2000: 00 00 00 0C 6A 50 20 20
    if bytes[..8] == [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20] {
        return ImageFormat::Jpeg2000;
    }

    // GIF: "GIF87a" or "GIF89a"
    if &bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a" {
        return ImageFormat::Gif;
    }

    // TIFF: little-endian (49 49 2A 00) or big-endian (4D 4D 00 2A)
    if bytes[..4] == [0x49, 0x49, 0x2A, 0x00] || bytes[..4] == [0x4D, 0x4D, 0x00, 0x2A] {
        return ImageFormat::Tiff;
    }

    // BMP: "BM"
    if bytes[0] == b'B' && bytes[1] == b'M' {
        return ImageFormat::Bmp;
    }

    // WebP: "RIFF" at offset 0 and "WEBP" at offset 8
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return ImageFormat::WebP;
    }

    ImageFormat::Unknown
}

/// Determine image format from a PDF stream filter name.
///
/// PDF uses filter names to indicate the compression/encoding of stream data.
/// `DCTDecode` corresponds to JPEG, `JPXDecode` to JPEG2000. All other
/// filters return `ImageFormat::Unknown`.
pub fn format_from_pdf_filter(filter_name: &str) -> ImageFormat {
    match filter_name {
        "DCTDecode" => ImageFormat::Jpeg,
        "JPXDecode" => ImageFormat::Jpeg2000,
        _ => ImageFormat::Unknown,
    }
}

/// Resolve the image format using a PDF filter name hint with magic byte
/// fallback.
///
/// If `filter` is `Some` and maps to a known format via
/// [`format_from_pdf_filter`], that format is returned. Otherwise, magic byte
/// detection via [`detect_image_format`] is applied to `raw_bytes`.
pub fn resolve_format(raw_bytes: &[u8], filter: Option<&str>) -> ImageFormat {
    if let Some(name) = filter {
        let from_filter = format_from_pdf_filter(name);
        if from_filter != ImageFormat::Unknown {
            return from_filter;
        }
    }
    detect_image_format(raw_bytes)
}

/// Build an `ImageData` from raw bytes, using an optional PDF filter name as
/// a format hint.
///
/// If the filter hint resolves to a known format, that is used directly.
/// Otherwise, magic byte detection is applied as a fallback.
pub fn build_image_data(id: ImageId, raw_bytes: Vec<u8>, filter: Option<&str>) -> ImageData {
    let format = resolve_format(&raw_bytes, filter);
    ImageData {
        id,
        format,
        bytes: raw_bytes,
    }
}

/// Extract a single image XObject from the PDF by its name.
///
/// Walks all pages in the document, inspecting each page's
/// `Resources -> XObject` dictionary to find a stream whose key matches
/// `id.as_str()`. Returns the decompressed stream bytes along with the
/// detected format.
///
/// Returns `PdfError::ImageNotFound` if no matching XObject is found.
pub fn extract_image(backend: &LopdfBackend, id: &ImageId) -> Result<ImageData, PdfError> {
    let doc = backend.raw_doc();
    let pages = doc.get_pages();
    let target_name = id.as_str().as_bytes();

    for &page_id in pages.values() {
        let page_obj = doc
            .get_object(page_id)
            .map_err(|e| PdfError::Parse(format!("cannot get page object: {}", e)))?;

        let page_dict = match page_obj.as_dict() {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Get the Resources dictionary, resolving a reference if needed.
        let resources_obj = match page_dict.get(b"Resources") {
            Ok(obj) => obj,
            Err(_) => continue,
        };

        let resources_dict = match resolve_dict(doc, resources_obj) {
            Some(d) => d,
            None => continue,
        };

        // Get the XObject dictionary from Resources.
        let xobject_obj = match resources_dict.get(b"XObject") {
            Ok(obj) => obj,
            Err(_) => continue,
        };

        let xobject_dict = match resolve_dict(doc, xobject_obj) {
            Some(d) => d,
            None => continue,
        };

        // Look for our target name in the XObject dictionary.
        let stream_obj = match xobject_dict.get(target_name) {
            Ok(obj) => obj,
            Err(_) => continue,
        };

        // Resolve reference if needed and extract the stream.
        let resolved = resolve_object(doc, stream_obj);
        if let Some(stream) = as_stream(resolved) {
            // Get the filter name for format detection.
            let filter_name = extract_filter_name(&stream.dict);

            // Get the decompressed content. Try to decode the stream first;
            // fall back to the raw content if decoding fails.
            let bytes = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());

            let image_data =
                build_image_data(ImageId::new(id.as_str()), bytes, filter_name.as_deref());

            return Ok(image_data);
        }
    }

    Err(PdfError::ImageNotFound(id.as_str().to_string()))
}

/// List all image XObjects on a given page.
///
/// Returns a `Vec<ImageRef>` with each image's name as its id, its detected
/// format, and `alt_text` set to `None`.
pub fn list_page_images(
    backend: &LopdfBackend,
    page_id: (u32, u16),
) -> Result<Vec<ImageRef>, PdfError> {
    let doc = backend.raw_doc();
    let mut images = Vec::new();

    let page_obj = doc
        .get_object(page_id)
        .map_err(|e| PdfError::Parse(format!("cannot get page object: {}", e)))?;

    let page_dict = page_obj
        .as_dict()
        .map_err(|e| PdfError::Parse(format!("page object is not a dictionary: {}", e)))?;

    // Get the Resources dictionary.
    let resources_obj = match page_dict.get(b"Resources") {
        Ok(obj) => obj,
        Err(_) => return Ok(images),
    };

    let resources_dict = match resolve_dict(doc, resources_obj) {
        Some(d) => d,
        None => return Ok(images),
    };

    // Get the XObject dictionary from Resources.
    let xobject_obj = match resources_dict.get(b"XObject") {
        Ok(obj) => obj,
        Err(_) => return Ok(images),
    };

    let xobject_dict = match resolve_dict(doc, xobject_obj) {
        Some(d) => d,
        None => return Ok(images),
    };

    // Iterate over all entries in the XObject dictionary.
    for (name, obj) in xobject_dict.iter() {
        let resolved = resolve_object(doc, obj);

        // Check if this XObject is an Image subtype.
        if let Some(stream) = as_stream(resolved) {
            let is_image = stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .is_some_and(|n| n == b"Image");

            if !is_image {
                continue;
            }

            let filter_name = extract_filter_name(&stream.dict);
            let format = resolve_format(&stream.content, filter_name.as_deref());

            let id_str = String::from_utf8_lossy(name).into_owned();
            images.push(ImageRef {
                id: ImageId::new(id_str),
                format,
                alt_text: None,
            });
        }
    }

    Ok(images)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Resolve a `lopdf::Object` that might be a `Reference` to the actual object.
fn resolve_object<'a>(doc: &'a lopdf::Document, obj: &'a lopdf::Object) -> &'a lopdf::Object {
    match obj {
        lopdf::Object::Reference(id) => doc.get_object(*id).unwrap_or(obj),
        _ => obj,
    }
}

/// Resolve an object to a `Dictionary`, following one level of reference
/// indirection if needed.
fn resolve_dict<'a>(
    doc: &'a lopdf::Document,
    obj: &'a lopdf::Object,
) -> Option<&'a lopdf::Dictionary> {
    match obj {
        lopdf::Object::Dictionary(d) => Some(d),
        lopdf::Object::Reference(id) => {
            let resolved = doc.get_object(*id).ok()?;
            resolved.as_dict().ok()
        }
        _ => None,
    }
}

/// Extract the stream from an object, if it is a `Stream`.
fn as_stream(obj: &lopdf::Object) -> Option<&lopdf::Stream> {
    match obj {
        lopdf::Object::Stream(s) => Some(s),
        _ => None,
    }
}

/// Extract the first filter name from a stream dictionary.
///
/// The `Filter` entry can be a single `Name` or an `Array` of names.
/// Returns the first filter name as a `String`, or `None` if not present.
fn extract_filter_name(dict: &lopdf::Dictionary) -> Option<String> {
    let filter_obj = dict.get(b"Filter").ok()?;
    match filter_obj {
        lopdf::Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        lopdf::Object::Array(arr) => arr.first().and_then(|o| match o {
            lopdf::Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
            _ => None,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- detect_image_format ------------------------------------------------

    #[test]
    fn detect_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Jpeg);
    }

    #[test]
    fn detect_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Png);
    }

    #[test]
    fn detect_gif87a() {
        let bytes = b"GIF87a\x00\x00\x00\x00";
        assert_eq!(detect_image_format(bytes), ImageFormat::Gif);
    }

    #[test]
    fn detect_gif89a() {
        let bytes = b"GIF89a\x00\x00\x00\x00";
        assert_eq!(detect_image_format(bytes), ImageFormat::Gif);
    }

    #[test]
    fn detect_tiff_little_endian() {
        let bytes = [0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Tiff);
    }

    #[test]
    fn detect_tiff_big_endian() {
        let bytes = [0x4D, 0x4D, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Tiff);
    }

    #[test]
    fn detect_bmp() {
        let bytes = b"BM\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(detect_image_format(bytes), ImageFormat::Bmp);
    }

    #[test]
    fn detect_webp() {
        let bytes = b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(detect_image_format(bytes), ImageFormat::WebP);
    }

    #[test]
    fn detect_jpeg2000() {
        let bytes = [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20, 0x0D, 0x0A];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Jpeg2000);
    }

    #[test]
    fn detect_unknown_bytes() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(detect_image_format(&bytes), ImageFormat::Unknown);
    }

    #[test]
    fn detect_short_input() {
        assert_eq!(detect_image_format(&[]), ImageFormat::Unknown);
        assert_eq!(detect_image_format(&[0xFF]), ImageFormat::Unknown);
        assert_eq!(detect_image_format(&[0xFF, 0xD8]), ImageFormat::Unknown);
        assert_eq!(
            detect_image_format(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A]),
            ImageFormat::Unknown
        );
    }

    // -- format_from_pdf_filter ---------------------------------------------

    #[test]
    fn filter_dct_decode() {
        assert_eq!(format_from_pdf_filter("DCTDecode"), ImageFormat::Jpeg);
    }

    #[test]
    fn filter_jpx_decode() {
        assert_eq!(format_from_pdf_filter("JPXDecode"), ImageFormat::Jpeg2000);
    }

    #[test]
    fn filter_flate_decode() {
        assert_eq!(format_from_pdf_filter("FlateDecode"), ImageFormat::Unknown);
    }

    #[test]
    fn filter_unknown_string() {
        assert_eq!(
            format_from_pdf_filter("SomethingElse"),
            ImageFormat::Unknown
        );
    }

    // -- build_image_data ---------------------------------------------------

    #[test]
    fn build_with_filter_hint_jpeg() {
        let id = ImageId::new("img1");
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let data = build_image_data(id, bytes.clone(), Some("DCTDecode"));
        assert_eq!(data.format, ImageFormat::Jpeg);
        assert_eq!(data.bytes, bytes);
        assert_eq!(data.id.as_str(), "img1");
    }

    #[test]
    fn build_with_filter_hint_jpeg2000() {
        let id = ImageId::new("img2");
        let bytes = vec![0x00; 16];
        let data = build_image_data(id, bytes, Some("JPXDecode"));
        assert_eq!(data.format, ImageFormat::Jpeg2000);
    }

    #[test]
    fn build_with_unknown_filter_falls_back_to_magic() {
        let id = ImageId::new("img3");
        // PNG magic bytes
        let bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        let data = build_image_data(id, bytes, Some("FlateDecode"));
        assert_eq!(data.format, ImageFormat::Png);
    }

    #[test]
    fn build_without_filter_uses_magic() {
        let id = ImageId::new("img4");
        // JPEG magic bytes
        let bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let data = build_image_data(id, bytes, None);
        assert_eq!(data.format, ImageFormat::Jpeg);
    }

    #[test]
    fn build_without_filter_unknown_bytes() {
        let id = ImageId::new("img5");
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let data = build_image_data(id, bytes, None);
        assert_eq!(data.format, ImageFormat::Unknown);
    }

    #[test]
    fn build_with_filter_hint_overrides_magic() {
        let id = ImageId::new("img6");
        // Bytes look like PNG, but filter says JPEG
        let bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        let data = build_image_data(id, bytes, Some("DCTDecode"));
        assert_eq!(data.format, ImageFormat::Jpeg);
    }
}
