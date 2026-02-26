use std::io::Cursor;

use crate::parser::backend::LopdfBackend;
use crate::types::{ImageData, ImageFormat, ImageId, ImageRef};
use crate::PdfError;

// ---------------------------------------------------------------------------
// Pure types for raw image handling
// ---------------------------------------------------------------------------

/// Parsed color space from a PDF image stream dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
}

/// Parsed image metadata from a PDF stream dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawImageMeta {
    width: u32,
    height: u32,
    bits_per_component: u8,
    channels: u8,
    color_space: ColorSpace,
}

impl RawImageMeta {
    /// Expected raw byte count for this image's pixel data.
    /// Accounts for sub-byte pixel packing with per-row byte alignment.
    fn expected_byte_count(&self) -> usize {
        let bits_per_row =
            self.width as usize * self.channels as usize * self.bits_per_component as usize;
        let bytes_per_row = bits_per_row.div_ceil(8);
        bytes_per_row * self.height as usize
    }
}

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

// ---------------------------------------------------------------------------
// Pure image conversion functions
// ---------------------------------------------------------------------------

/// Parse image metadata from a PDF stream dictionary.
fn extract_image_meta(dict: &lopdf::Dictionary) -> Option<RawImageMeta> {
    let width = dict.get(b"Width").ok()?.as_i64().ok()? as u32;
    let height = dict.get(b"Height").ok()?.as_i64().ok()? as u32;

    let bits_per_component = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|obj| obj.as_i64().ok())
        .map(|v| v as u8)
        .unwrap_or(8);

    let color_space_name = dict.get(b"ColorSpace").ok()?.as_name().ok()?;
    let (color_space, channels) = match color_space_name {
        b"DeviceRGB" => (ColorSpace::Rgb, 3),
        b"DeviceGray" => (ColorSpace::Gray, 1),
        b"DeviceCMYK" => (ColorSpace::Cmyk, 4),
        _ => return None,
    };

    Some(RawImageMeta {
        width,
        height,
        bits_per_component,
        channels,
        color_space,
    })
}

/// Re-encode raw pixel data as PNG.
fn encode_raw_as_png(meta: &RawImageMeta, raw_bytes: &[u8]) -> Option<Vec<u8>> {
    if raw_bytes.len() != meta.expected_byte_count() {
        return None;
    }

    let expanded = if meta.bits_per_component < 8 {
        expand_sub_byte_pixels(raw_bytes, meta)
    } else {
        raw_bytes.to_vec()
    };

    let dyn_image = match meta.color_space {
        ColorSpace::Gray => {
            let img = image::GrayImage::from_raw(meta.width, meta.height, expanded)?;
            image::DynamicImage::ImageLuma8(img)
        }
        ColorSpace::Rgb => {
            let img = image::RgbImage::from_raw(meta.width, meta.height, expanded)?;
            image::DynamicImage::ImageRgb8(img)
        }
        ColorSpace::Cmyk => {
            let rgb_pixels = cmyk_to_rgb(&expanded);
            let img = image::RgbImage::from_raw(meta.width, meta.height, rgb_pixels)?;
            image::DynamicImage::ImageRgb8(img)
        }
    };

    let mut buf = Vec::new();
    dyn_image
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}

/// Expand sub-byte packed pixels (1-bit, 2-bit, 4-bit) to 8-bit per component.
fn expand_sub_byte_pixels(raw_bytes: &[u8], meta: &RawImageMeta) -> Vec<u8> {
    let pixels_per_row = meta.width as usize * meta.channels as usize;
    let bits_per_row = pixels_per_row * meta.bits_per_component as usize;
    let bytes_per_row = bits_per_row.div_ceil(8);
    let bpc = meta.bits_per_component;
    let max_val = (1u16 << bpc) - 1;

    let mut result = Vec::with_capacity(pixels_per_row * meta.height as usize);

    for row in 0..meta.height as usize {
        let row_start = row * bytes_per_row;
        let row_bytes = &raw_bytes[row_start..row_start + bytes_per_row];
        let mut pixel_count = 0;

        for &byte in row_bytes {
            let pixels_in_byte = 8 / bpc as usize;
            for i in 0..pixels_in_byte {
                if pixel_count >= pixels_per_row {
                    break;
                }
                let shift = 8 - bpc * (i as u8 + 1);
                let val = (byte >> shift) & (max_val as u8);
                let scaled = (val as u16 * 255 / max_val) as u8;
                result.push(scaled);
                pixel_count += 1;
            }
        }
    }

    result
}

/// Convert CMYK pixel bytes to RGB.
fn cmyk_to_rgb(cmyk_bytes: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(cmyk_bytes.len() / 4 * 3);
    for pixel in cmyk_bytes.chunks_exact(4) {
        let (c, m, y, k) = (
            pixel[0] as u16,
            pixel[1] as u16,
            pixel[2] as u16,
            pixel[3] as u16,
        );
        let r = 255u16.saturating_sub((c + k).min(255)) as u8;
        let g = 255u16.saturating_sub((m + k).min(255)) as u8;
        let b = 255u16.saturating_sub((y + k).min(255)) as u8;
        rgb.extend_from_slice(&[r, g, b]);
    }
    rgb
}

/// Decode CCITT Group 4 fax data into a PNG image.
fn decode_ccitt(dict: &lopdf::Dictionary, raw_bytes: &[u8]) -> Option<Vec<u8>> {
    let decode_parms = extract_decode_parms(dict)?;

    let width = decode_parms.get(b"Columns").ok()?.as_i64().ok()? as u16;
    let height = decode_parms
        .get(b"Rows")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|v| v as u16);

    let k = decode_parms
        .get(b"K")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);

    if k >= 0 {
        return None;
    }

    let bytes_per_row = (width as usize).div_ceil(8);
    let mut rows: Vec<Vec<u8>> = Vec::new();

    fax::decoder::decode_g4(raw_bytes.iter().copied(), width, height, |transitions| {
        let mut row = pack_row_bits(transitions, width);
        row.resize(bytes_per_row, 0);
        rows.push(row);
    })?;

    if rows.is_empty() {
        return None;
    }

    let pixel_data: Vec<u8> = rows.into_iter().flatten().collect();

    let meta = RawImageMeta {
        width: width as u32,
        height: pixel_data.len() as u32 / bytes_per_row as u32,
        bits_per_component: 1,
        channels: 1,
        color_space: ColorSpace::Gray,
    };

    encode_raw_as_png(&meta, &pixel_data)
}

/// Extract the DecodeParms dictionary from a stream dictionary.
fn extract_decode_parms(dict: &lopdf::Dictionary) -> Option<&lopdf::Dictionary> {
    let obj = dict.get(b"DecodeParms").ok()?;
    match obj {
        lopdf::Object::Dictionary(d) => Some(d),
        lopdf::Object::Array(arr) => arr.first().and_then(|o| o.as_dict().ok()),
        _ => None,
    }
}

/// Convert fax transition positions into a packed 1-bit byte array.
fn pack_row_bits(transitions: &[u16], width: u16) -> Vec<u8> {
    let bytes_per_row = (width as usize).div_ceil(8);
    let mut row = vec![0u8; bytes_per_row];

    let mut set_black_run = |start: u16, end: u16| {
        for col in start..end.min(width) {
            let byte_idx = col as usize / 8;
            let bit_idx = 7 - (col as usize % 8);
            row[byte_idx] |= 1 << bit_idx;
        }
    };

    let mut is_black = false;
    let mut prev_pos: u16 = 0;

    for &pos in transitions {
        if is_black {
            set_black_run(prev_pos, pos);
        }
        prev_pos = pos;
        is_black = !is_black;
    }

    if is_black {
        set_black_run(prev_pos, width);
    }

    row
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

        let xobject_dict = match resolve_xobject_dict(doc, page_dict) {
            Some(d) => d,
            None => continue,
        };

        let stream_obj = match xobject_dict.get(target_name) {
            Ok(obj) => obj,
            Err(_) => continue,
        };

        let resolved = resolve_object(doc, stream_obj);
        let Some(stream) = as_stream(resolved) else {
            continue;
        };

        let filter_name = extract_filter_name(&stream.dict);

        // CCITTFaxDecode: lopdf cannot decompress this -- decode from raw stream
        if filter_name.as_deref() == Some("CCITTFaxDecode") {
            if let Some(png_bytes) = decode_ccitt(&stream.dict, &stream.content) {
                return Ok(ImageData {
                    id: ImageId::new(id.as_str()),
                    format: ImageFormat::Png,
                    bytes: png_bytes,
                });
            }
        }

        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let image_data = build_image_data(ImageId::new(id.as_str()), bytes, filter_name.as_deref());

        // Raw pixel data fallback: try re-encoding as PNG
        if image_data.format == ImageFormat::Unknown {
            if let Some(meta) = extract_image_meta(&stream.dict) {
                if let Some(png_bytes) = encode_raw_as_png(&meta, &image_data.bytes) {
                    return Ok(ImageData {
                        id: image_data.id,
                        format: ImageFormat::Png,
                        bytes: png_bytes,
                    });
                }
            }
        }

        return Ok(image_data);
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

    let xobject_dict = match resolve_xobject_dict(doc, page_dict) {
        Some(d) => d,
        None => return Ok(images),
    };

    for (name, obj) in xobject_dict.iter() {
        let resolved = resolve_object(doc, obj);
        let Some(stream) = as_stream(resolved) else {
            continue;
        };

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
        let mut format = resolve_format(&stream.content, filter_name.as_deref());

        if format == ImageFormat::Unknown
            && can_reencode_as_png(filter_name.as_deref(), &stream.dict)
        {
            format = ImageFormat::Png;
        }

        let id_str = String::from_utf8_lossy(name).into_owned();
        images.push(ImageRef {
            id: ImageId::new(id_str),
            format,
            alt_text: None,
        });
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

/// Resolve the XObject dictionary from a page dictionary, following references
/// through Resources -> XObject.
fn resolve_xobject_dict<'a>(
    doc: &'a lopdf::Document,
    page_dict: &'a lopdf::Dictionary,
) -> Option<&'a lopdf::Dictionary> {
    let resources_obj = page_dict.get(b"Resources").ok()?;
    let resources_dict = resolve_dict(doc, resources_obj)?;
    let xobject_obj = resources_dict.get(b"XObject").ok()?;
    resolve_dict(doc, xobject_obj)
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

/// Check whether raw pixel data can be re-encoded as PNG.
///
/// Returns true when the stream has either a CCITTFaxDecode filter (fax
/// image) or parseable image metadata (DeviceRGB/Gray/CMYK with known
/// dimensions).
fn can_reencode_as_png(filter_name: Option<&str>, dict: &lopdf::Dictionary) -> bool {
    filter_name == Some("CCITTFaxDecode") || extract_image_meta(dict).is_some()
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

    // -- extract_image_meta -------------------------------------------------

    #[test]
    fn extract_meta_rgb() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(100));
        dict.set("Height", lopdf::Object::Integer(50));
        dict.set("BitsPerComponent", lopdf::Object::Integer(8));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
        let meta = extract_image_meta(&dict).unwrap();
        assert_eq!(meta.width, 100);
        assert_eq!(meta.height, 50);
        assert_eq!(meta.bits_per_component, 8);
        assert_eq!(meta.channels, 3);
        assert_eq!(meta.color_space, ColorSpace::Rgb);
    }

    #[test]
    fn extract_meta_gray() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(200));
        dict.set("Height", lopdf::Object::Integer(100));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceGray".to_vec()));
        let meta = extract_image_meta(&dict).unwrap();
        assert_eq!(meta.channels, 1);
        assert_eq!(meta.color_space, ColorSpace::Gray);
    }

    #[test]
    fn extract_meta_cmyk() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(50));
        dict.set("Height", lopdf::Object::Integer(50));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceCMYK".to_vec()));
        let meta = extract_image_meta(&dict).unwrap();
        assert_eq!(meta.channels, 4);
        assert_eq!(meta.color_space, ColorSpace::Cmyk);
    }

    #[test]
    fn extract_meta_missing_width() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Height", lopdf::Object::Integer(50));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
        assert!(extract_image_meta(&dict).is_none());
    }

    #[test]
    fn extract_meta_missing_height() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(100));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
        assert!(extract_image_meta(&dict).is_none());
    }

    #[test]
    fn extract_meta_unsupported_colorspace() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(100));
        dict.set("Height", lopdf::Object::Integer(50));
        dict.set("ColorSpace", lopdf::Object::Name(b"ICCBased".to_vec()));
        assert!(extract_image_meta(&dict).is_none());
    }

    #[test]
    fn extract_meta_default_bits() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Width", lopdf::Object::Integer(100));
        dict.set("Height", lopdf::Object::Integer(50));
        dict.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
        let meta = extract_image_meta(&dict).unwrap();
        assert_eq!(meta.bits_per_component, 8);
    }

    // -- expected_byte_count ------------------------------------------------

    #[test]
    fn byte_count_8bit_rgb() {
        let meta = RawImageMeta {
            width: 10,
            height: 5,
            bits_per_component: 8,
            channels: 3,
            color_space: ColorSpace::Rgb,
        };
        assert_eq!(meta.expected_byte_count(), 150);
    }

    #[test]
    fn byte_count_1bit_gray() {
        let meta = RawImageMeta {
            width: 8,
            height: 1,
            bits_per_component: 1,
            channels: 1,
            color_space: ColorSpace::Gray,
        };
        assert_eq!(meta.expected_byte_count(), 1);
    }

    #[test]
    fn byte_count_1bit_gray_with_padding() {
        let meta = RawImageMeta {
            width: 10,
            height: 1,
            bits_per_component: 1,
            channels: 1,
            color_space: ColorSpace::Gray,
        };
        assert_eq!(meta.expected_byte_count(), 2);
    }

    #[test]
    fn byte_count_4bit_gray() {
        let meta = RawImageMeta {
            width: 3,
            height: 2,
            bits_per_component: 4,
            channels: 1,
            color_space: ColorSpace::Gray,
        };
        assert_eq!(meta.expected_byte_count(), 4);
    }

    // -- encode_raw_as_png --------------------------------------------------

    const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn encode_rgb_png() {
        let meta = RawImageMeta {
            width: 2,
            height: 2,
            bits_per_component: 8,
            channels: 3,
            color_space: ColorSpace::Rgb,
        };
        let raw = vec![0u8; 12];
        let png = encode_raw_as_png(&meta, &raw).unwrap();
        assert_eq!(png[..8], PNG_MAGIC);
    }

    #[test]
    fn encode_gray_png() {
        let meta = RawImageMeta {
            width: 2,
            height: 2,
            bits_per_component: 8,
            channels: 1,
            color_space: ColorSpace::Gray,
        };
        let raw = vec![0u8; 4];
        let png = encode_raw_as_png(&meta, &raw).unwrap();
        assert_eq!(png[..8], PNG_MAGIC);
    }

    #[test]
    fn encode_cmyk_png() {
        let meta = RawImageMeta {
            width: 2,
            height: 2,
            bits_per_component: 8,
            channels: 4,
            color_space: ColorSpace::Cmyk,
        };
        let raw = vec![0u8; 16];
        let png = encode_raw_as_png(&meta, &raw).unwrap();
        assert_eq!(png[..8], PNG_MAGIC);
    }

    #[test]
    fn encode_1bit_png() {
        let meta = RawImageMeta {
            width: 8,
            height: 1,
            bits_per_component: 1,
            channels: 1,
            color_space: ColorSpace::Gray,
        };
        let raw = vec![0xFFu8];
        let png = encode_raw_as_png(&meta, &raw).unwrap();
        assert_eq!(png[..8], PNG_MAGIC);
    }

    #[test]
    fn encode_size_mismatch_returns_none() {
        let meta = RawImageMeta {
            width: 2,
            height: 2,
            bits_per_component: 8,
            channels: 3,
            color_space: ColorSpace::Rgb,
        };
        let raw = vec![0u8; 10]; // expects 12
        assert!(encode_raw_as_png(&meta, &raw).is_none());
    }

    #[test]
    fn encode_empty_bytes_returns_none() {
        let meta = RawImageMeta {
            width: 2,
            height: 2,
            bits_per_component: 8,
            channels: 3,
            color_space: ColorSpace::Rgb,
        };
        assert!(encode_raw_as_png(&meta, &[]).is_none());
    }

    // -- cmyk_to_rgb --------------------------------------------------------

    #[test]
    fn cmyk_all_zeros_gives_white() {
        let cmyk = vec![0, 0, 0, 0];
        let rgb = cmyk_to_rgb(&cmyk);
        assert_eq!(rgb, vec![255, 255, 255]);
    }

    #[test]
    fn cmyk_full_black() {
        let cmyk = vec![0, 0, 0, 255];
        let rgb = cmyk_to_rgb(&cmyk);
        assert_eq!(rgb, vec![0, 0, 0]);
    }

    #[test]
    fn cmyk_pure_cyan() {
        let cmyk = vec![255, 0, 0, 0];
        let rgb = cmyk_to_rgb(&cmyk);
        assert_eq!(rgb, vec![0, 255, 255]);
    }

    // -- pack_row_bits ------------------------------------------------------

    #[test]
    fn pack_all_white() {
        let row = pack_row_bits(&[], 8);
        assert_eq!(row, vec![0x00]);
    }

    #[test]
    fn pack_all_black() {
        let row = pack_row_bits(&[0], 8);
        assert_eq!(row, vec![0xFF]);
    }

    #[test]
    fn pack_half_and_half() {
        // First 4 pixels white, last 4 black
        let row = pack_row_bits(&[4], 8);
        assert_eq!(row, vec![0x0F]);
    }

    // -- decode_ccitt -------------------------------------------------------

    #[test]
    fn decode_ccitt_missing_decode_parms() {
        let dict = lopdf::Dictionary::new();
        assert!(decode_ccitt(&dict, &[]).is_none());
    }

    #[test]
    fn decode_ccitt_group3_returns_none() {
        let mut parms = lopdf::Dictionary::new();
        parms.set("Columns", lopdf::Object::Integer(100));
        parms.set("Rows", lopdf::Object::Integer(10));
        parms.set("K", lopdf::Object::Integer(0));
        let mut dict = lopdf::Dictionary::new();
        dict.set("DecodeParms", lopdf::Object::Dictionary(parms));
        assert!(decode_ccitt(&dict, &[0x00]).is_none());
    }
}
