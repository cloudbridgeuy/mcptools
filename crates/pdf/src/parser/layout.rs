//! Text extraction, line grouping, heading detection, and block assembly.
//!
//! This module implements a pure-functional pipeline that transforms raw PDF
//! content-stream operators into structured [`TextBlock`]s with heading
//! classification.  Every public function is a pure transformation -- side
//! effects (I/O) live behind the [`PdfBackend`] trait provided by the caller.
//!
//! # Pipeline
//!
//! ```text
//! content ops  ->  TextSpan[]  ->  TextLine[]  ->  TextBlock[]
//!   (per page)      extract         group_spans      group_lines
//!                                   detect_headings
//! ```

use std::collections::HashMap;

use super::backend::{get_number_from_value, BackendFontInfo, PageId, PdfBackend, PdfValue};
use crate::PdfError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single run of text at a specific position on the page.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub font_size: f32,
    pub font_name: String,
    pub is_bold: bool,
    pub is_italic: bool,
}

/// A horizontal line of text assembled from one or more [`TextSpan`]s that
/// share (approximately) the same Y coordinate.
#[derive(Debug, Clone)]
pub struct TextLine {
    pub spans: Vec<TextSpan>,
    pub y: f32,
    pub x: f32,
    pub font_size: f32,
    pub is_heading: bool,
    pub heading_level: u8,
}

impl TextLine {
    /// Concatenate all span texts with a single space separator.
    pub fn text(&self) -> String {
        self.spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Default for TextLine {
    fn default() -> Self {
        Self {
            spans: Vec::new(),
            y: 0.0,
            x: 0.0,
            font_size: 0.0,
            is_heading: false,
            heading_level: 0,
        }
    }
}

/// A vertical group of consecutive [`TextLine`]s sharing a block type.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub lines: Vec<TextLine>,
    pub block_type: BlockType,
}

/// Classification of a [`TextBlock`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockType {
    Paragraph,
    Heading(u8),
    ListItem,
    Table,
}

/// Aggregate font-size statistics computed across an entire document.
#[derive(Debug, Clone)]
pub struct FontStatistics {
    /// The most common font size (weighted by character count).
    pub body_size: f32,
    /// Sizes strictly above this value are considered headings.
    pub heading_threshold: f32,
    /// `(font_size, total_char_count)` pairs sorted by descending size.
    pub size_histogram: Vec<(f32, usize)>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Two spans whose Y coordinates differ by less than this are treated as
/// belonging to the same line.
const Y_TOLERANCE: f32 = 1.0;

/// Approximate character width as a fraction of font size when no better
/// metric is available.  0.5 is a reasonable default for proportional fonts.
const APPROX_CHAR_WIDTH_RATIO: f32 = 0.5;

/// Minimum gap (in points) between adjacent spans before we insert a space.
const MIN_WORD_GAP: f32 = 1.5;

/// When grouping lines into blocks, a vertical gap larger than this multiple
/// of the line's font size starts a new block.
const BLOCK_GAP_FACTOR: f32 = 1.4;

/// Quantisation bucket width for font sizes in the histogram (points).
const FONT_SIZE_BUCKET: f32 = 0.5;

// ---------------------------------------------------------------------------
// CJK / spaceless-script helper
// ---------------------------------------------------------------------------

/// Returns `true` if `c` belongs to a script that does not use inter-word
/// spaces (CJK Unified Ideographs, Hiragana, Katakana, Hangul, Thai, etc.).
pub fn is_spaceless_script_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF
        // CJK Unified Ideographs Extension A
        | 0x3400..=0x4DBF
        // CJK Unified Ideographs Extension B
        | 0x20000..=0x2A6DF
        // CJK Compatibility Ideographs
        | 0xF900..=0xFAFF
        // Hiragana
        | 0x3040..=0x309F
        // Katakana
        | 0x30A0..=0x30FF
        // Katakana Phonetic Extensions
        | 0x31F0..=0x31FF
        // Hangul Syllables
        | 0xAC00..=0xD7AF
        // Hangul Jamo
        | 0x1100..=0x11FF
        // Hangul Compatibility Jamo
        | 0x3130..=0x318F
        // CJK Symbols and Punctuation
        | 0x3000..=0x303F
        // Fullwidth Forms
        | 0xFF00..=0xFFEF
        // Thai
        | 0x0E00..=0x0E7F
        // Lao
        | 0x0E80..=0x0EFF
        // Myanmar
        | 0x1000..=0x109F
        // Khmer
        | 0x1780..=0x17FF
        // Tibetan
        | 0x0F00..=0x0FFF
    )
}

// ---------------------------------------------------------------------------
// Internal: PDF text-state machine
// ---------------------------------------------------------------------------

/// Mutable state tracked while walking a page's content stream.
#[derive(Debug, Clone)]
struct TextState {
    /// Current font resource name (the `/F1`-style key, not the full name).
    font_key: Vec<u8>,
    /// Resolved base-font name for the current font.
    font_name: String,
    /// Current font size in text-space units.
    font_size: f32,
    /// Elements [a, b, c, d, tx, ty] of the current text matrix.
    text_matrix: [f32; 6],
    /// Text line matrix -- set by BT and updated by Td/TD/T*/Tm.
    line_matrix: [f32; 6],
    /// Horizontal scaling factor (percent / 100).  Default 1.0.
    horiz_scale: f32,
    /// Character spacing (Tc).
    char_spacing: f32,
    /// Word spacing (Tw).
    word_spacing: f32,
    /// Text rise (Ts).
    text_rise: f32,
    /// Leading (TL).
    leading: f32,
    /// Is bold detected from the font name?
    is_bold: bool,
    /// Is italic detected from the font name?
    is_italic: bool,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font_key: Vec::new(),
            font_name: String::new(),
            font_size: 0.0,
            text_matrix: IDENTITY_MATRIX,
            line_matrix: IDENTITY_MATRIX,
            horiz_scale: 1.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            text_rise: 0.0,
            leading: 0.0,
            is_bold: false,
            is_italic: false,
        }
    }
}

/// The identity 2x3 text matrix: [a, b, c, d, tx, ty].
const IDENTITY_MATRIX: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

impl TextState {
    /// Current X position derived from the text matrix.
    fn x(&self) -> f32 {
        self.text_matrix[4]
    }

    /// Current Y position derived from the text matrix.
    fn y(&self) -> f32 {
        self.text_matrix[5]
    }

    /// Effective font size accounting for the text matrix vertical scale.
    ///
    /// The rendered size is `font_size * sqrt(b^2 + d^2)` where b and d are
    /// elements [1] and [3] of the text matrix respectively.
    fn effective_font_size(&self) -> f32 {
        let scale = (self.text_matrix[1].powi(2) + self.text_matrix[3].powi(2)).sqrt();
        (self.font_size * scale).abs()
    }

    /// Advance the text matrix horizontally by `dx` text-space units.
    fn advance_x(&mut self, dx: f32) {
        self.text_matrix[4] += dx * self.text_matrix[0];
        self.text_matrix[5] += dx * self.text_matrix[1];
    }

    /// Multiply the text line matrix by a translation (used by Td / TD).
    fn translate_line(&mut self, tx: f32, ty: f32) {
        let new_tx = self.line_matrix[0] * tx + self.line_matrix[2] * ty + self.line_matrix[4];
        let new_ty = self.line_matrix[1] * tx + self.line_matrix[3] * ty + self.line_matrix[5];
        self.line_matrix[4] = new_tx;
        self.line_matrix[5] = new_ty;
        self.text_matrix = self.line_matrix;
    }

    /// Apply the `Tf` operator: set font and size, detect bold/italic from
    /// the base-font name.
    fn set_font(&mut self, key: Vec<u8>, base_font: &str, size: f32) {
        self.font_key = key;
        self.font_size = size;

        let upper = base_font.to_uppercase();
        self.is_bold = upper.contains("BOLD");
        self.is_italic = upper.contains("ITALIC") || upper.contains("OBLIQUE");
        self.font_name = base_font.to_string();
    }
}

/// Resolve a font resource name to its [`BackendFontInfo`].
///
/// The `BackendFontInfo.name` field holds the resource key (e.g. `b"F1"`).
fn resolve_font<'a>(key: &[u8], fonts: &'a [BackendFontInfo]) -> Option<&'a BackendFontInfo> {
    fonts.iter().find(|info| info.name == key)
}

/// Estimate the rendered width of a text string given the current state.
///
/// Since we do not have access to the actual glyph metrics (widths array) in
/// the pure-functional core, we approximate: each character contributes
/// `font_size * APPROX_CHAR_WIDTH_RATIO * horiz_scale`.
fn estimate_text_width(text: &str, state: &TextState) -> f32 {
    let n = text.chars().count() as f32;
    n * state.font_size * APPROX_CHAR_WIDTH_RATIO * state.horiz_scale
}

/// Advance the text matrix after rendering `text` and return the horizontal
/// displacement that was applied.
fn advance_after_show(text: &str, state: &mut TextState) -> f32 {
    let mut total_dx: f32 = 0.0;
    for ch in text.chars() {
        let char_w = state.font_size * APPROX_CHAR_WIDTH_RATIO * state.horiz_scale;
        total_dx += char_w + state.char_spacing;
        if ch == ' ' {
            total_dx += state.word_spacing;
        }
    }
    state.advance_x(total_dx);
    total_dx
}

/// Decode a single [`PdfValue::Str`] operand into a `String`, using the
/// backend's font-aware decoder.
fn decode_string(
    val: &PdfValue,
    backend: &dyn PdfBackend,
    page_id: PageId,
    font_key: &[u8],
) -> String {
    match val {
        PdfValue::Str(bytes) => {
            let decoded = backend.decode_text(page_id, font_key, bytes);
            if decoded.is_empty() {
                super::backend::decode_text_simple(bytes)
            } else {
                decoded
            }
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Public API: span extraction
// ---------------------------------------------------------------------------

/// Walk a single page's content stream and produce a flat list of
/// [`TextSpan`]s.
///
/// This is the heart of the text extraction pipeline.  It implements a
/// simplified PDF text-rendering state machine handling the operators:
///
/// | Operator | Action |
/// |----------|--------|
/// | `BT`     | Begin text object -- reset matrices |
/// | `ET`     | End text object |
/// | `Tf`     | Set font and size |
/// | `Tm`     | Set text matrix directly |
/// | `Td`     | Translate text position |
/// | `TD`     | Translate and set leading |
/// | `T*`     | Move to start of next line |
/// | `TL`     | Set text leading |
/// | `Tc`     | Set character spacing |
/// | `Tw`     | Set word spacing |
/// | `Tz`     | Set horizontal scaling |
/// | `Ts`     | Set text rise |
/// | `Tj`     | Show a string |
/// | `TJ`     | Show strings with kerning adjustments |
/// | `'`      | Move to next line and show string |
/// | `"`      | Set spacing, move to next line and show string |
pub fn extract_page_spans(
    backend: &dyn PdfBackend,
    page_id: PageId,
) -> Result<Vec<TextSpan>, PdfError> {
    // Get raw content bytes and decode into operations.
    let raw_content = backend.page_content(page_id)?;
    let ops = backend.decode_content(&raw_content)?;
    let fonts = backend.page_fonts(page_id).unwrap_or_default();

    let mut state = TextState::default();
    let mut spans: Vec<TextSpan> = Vec::new();

    for op in &ops {
        match op.operator.as_str() {
            // -- Text object delimiters --------------------------------
            "BT" => {
                state.text_matrix = IDENTITY_MATRIX;
                state.line_matrix = IDENTITY_MATRIX;
            }
            "ET" => {
                // Nothing to reset -- we keep font state across text objects
                // because some PDFs reuse the font set earlier.
            }

            // -- Font ---------------------------------------------------
            "Tf" => {
                handle_tf(&op.operands, &fonts, &mut state);
            }

            // -- Text matrix / position ---------------------------------
            "Tm" => {
                handle_tm(&op.operands, &mut state);
            }
            "Td" => {
                if op.operands.len() >= 2 {
                    let tx = get_number_from_value(&op.operands[0]).unwrap_or(0.0);
                    let ty = get_number_from_value(&op.operands[1]).unwrap_or(0.0);
                    state.translate_line(tx, ty);
                }
            }
            "TD" => {
                // TD is equivalent to: -ty TL ; tx ty Td
                if op.operands.len() >= 2 {
                    let tx = get_number_from_value(&op.operands[0]).unwrap_or(0.0);
                    let ty = get_number_from_value(&op.operands[1]).unwrap_or(0.0);
                    state.leading = -ty;
                    state.translate_line(tx, ty);
                }
            }
            "T*" => {
                // Move to start of next line: equivalent to 0 -TL Td
                state.translate_line(0.0, -state.leading);
            }
            "TL" => {
                if let Some(v) = op.operands.first().and_then(get_number_from_value) {
                    state.leading = v;
                }
            }

            // -- Spacing / scaling --------------------------------------
            "Tc" => {
                if let Some(v) = op.operands.first().and_then(get_number_from_value) {
                    state.char_spacing = v;
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first().and_then(get_number_from_value) {
                    state.word_spacing = v;
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first().and_then(get_number_from_value) {
                    state.horiz_scale = v / 100.0;
                }
            }
            "Ts" => {
                if let Some(v) = op.operands.first().and_then(get_number_from_value) {
                    state.text_rise = v;
                }
            }

            // -- Show text ----------------------------------------------
            "Tj" => {
                if let Some(first) = op.operands.first() {
                    emit_show_string(first, backend, page_id, &mut state, &mut spans);
                }
            }
            "TJ" => {
                if let Some(PdfValue::Array(arr)) = op.operands.first() {
                    handle_tj_array(arr, backend, page_id, &mut state, &mut spans);
                }
            }

            // -- Convenience show operators -----------------------------
            "'" => {
                // Move to next line, then show string.
                state.translate_line(0.0, -state.leading);
                if let Some(first) = op.operands.first() {
                    emit_show_string(first, backend, page_id, &mut state, &mut spans);
                }
            }
            "\"" => {
                // " aw ac string  =>  set Tw, Tc, T*, Tj
                if op.operands.len() >= 3 {
                    if let Some(aw) = get_number_from_value(&op.operands[0]) {
                        state.word_spacing = aw;
                    }
                    if let Some(ac) = get_number_from_value(&op.operands[1]) {
                        state.char_spacing = ac;
                    }
                    state.translate_line(0.0, -state.leading);
                    emit_show_string(&op.operands[2], backend, page_id, &mut state, &mut spans);
                }
            }

            _ => { /* Ignore non-text operators */ }
        }
    }

    Ok(spans)
}

/// Handle the `Tf` (set font) operator.
fn handle_tf(operands: &[PdfValue], fonts: &[BackendFontInfo], state: &mut TextState) {
    if operands.len() < 2 {
        return;
    }
    let key = match &operands[0] {
        PdfValue::Name(n) => n.clone(),
        PdfValue::Str(s) => s.clone(),
        _ => return,
    };
    let size = get_number_from_value(&operands[1]).unwrap_or(0.0);
    if let Some(info) = resolve_font(&key, fonts) {
        let base = info.base_font.as_deref().unwrap_or("");
        state.set_font(key, base, size);
    } else {
        // Font not in resource dict -- keep the key anyway.
        let name = String::from_utf8_lossy(&key).to_string();
        state.set_font(key, &name, size);
    }
}

/// Handle the `Tm` (set text matrix) operator.
fn handle_tm(operands: &[PdfValue], state: &mut TextState) {
    if operands.len() < 6 {
        return;
    }
    let vals: Vec<f32> = operands
        .iter()
        .take(6)
        .filter_map(get_number_from_value)
        .collect();
    if vals.len() == 6 {
        state.text_matrix = [vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]];
        state.line_matrix = state.text_matrix;
    }
}

/// Decode an operand as a string, create a [`TextSpan`], and advance the
/// text position.  Shared by `Tj`, `'`, and `"` operators.
fn emit_show_string(
    operand: &PdfValue,
    backend: &dyn PdfBackend,
    page_id: PageId,
    state: &mut TextState,
    spans: &mut Vec<TextSpan>,
) {
    let text = decode_string(operand, backend, page_id, &state.font_key);
    if text.is_empty() {
        return;
    }
    let x = state.x();
    let y = state.y() + state.text_rise;
    let fs = state.effective_font_size();
    let width = estimate_text_width(&text, state);
    spans.push(TextSpan {
        text: text.clone(),
        x,
        y,
        width,
        font_size: fs,
        font_name: state.font_name.clone(),
        is_bold: state.is_bold,
        is_italic: state.is_italic,
    });
    advance_after_show(&text, state);
}

/// Process a `TJ` array: elements are either strings to render or numeric
/// kerning adjustments (in thousandths of a unit of text space).
fn handle_tj_array(
    arr: &[PdfValue],
    backend: &dyn PdfBackend,
    page_id: PageId,
    state: &mut TextState,
    spans: &mut Vec<TextSpan>,
) {
    // Accumulate text fragments and emit a single span per contiguous run of
    // strings, splitting on large kerning adjustments that indicate word
    // boundaries.
    let mut buf = String::new();
    let mut span_x = state.x();
    let span_y = state.y() + state.text_rise;

    for elem in arr {
        match elem {
            PdfValue::Str(_) => {
                let fragment = decode_string(elem, backend, page_id, &state.font_key);
                if buf.is_empty() {
                    span_x = state.x();
                }
                buf.push_str(&fragment);
                advance_after_show(&fragment, state);
            }
            val => {
                // Numeric kerning: negative value = move right, positive =
                // move left (in thousandths of a text-space unit).
                if let Some(adj) = get_number_from_value(val) {
                    let dx = -adj / 1000.0 * state.font_size * state.horiz_scale;

                    // If the displacement is large enough to look like a word
                    // gap, insert a space character into the accumulated buffer.
                    let gap_threshold =
                        state.font_size * APPROX_CHAR_WIDTH_RATIO * state.horiz_scale * 0.3;

                    if dx > gap_threshold && !buf.is_empty() {
                        buf.push(' ');
                    }

                    state.advance_x(dx);
                }
            }
        }
    }

    // Flush remaining buffer.
    flush_tj_buffer(&mut buf, span_x, span_y, state, spans);
}

/// Flush the accumulated TJ string buffer into a [`TextSpan`].
fn flush_tj_buffer(
    buf: &mut String,
    span_x: f32,
    span_y: f32,
    state: &TextState,
    spans: &mut Vec<TextSpan>,
) {
    let trimmed = buf.trim_end();
    if trimmed.is_empty() {
        buf.clear();
        return;
    }
    let fs = state.effective_font_size();
    let width = estimate_text_width(trimmed, state);
    spans.push(TextSpan {
        text: trimmed.to_string(),
        x: span_x,
        y: span_y,
        width,
        font_size: fs,
        font_name: state.font_name.clone(),
        is_bold: state.is_bold,
        is_italic: state.is_italic,
    });
    buf.clear();
}

// ---------------------------------------------------------------------------
// Public API: multi-page extraction
// ---------------------------------------------------------------------------

/// Extract text spans from every page in the document.
///
/// Returns a `Vec` of `(page_number, spans)` where `page_number` is the
/// 1-based index from the backend's page map.
pub fn extract_all_pages(
    backend: &dyn PdfBackend,
) -> Result<Vec<(usize, Vec<TextSpan>)>, PdfError> {
    let page_map = backend.pages();
    let mut result: Vec<(usize, Vec<TextSpan>)> = Vec::with_capacity(page_map.len());

    for (&page_num, &page_id) in &page_map {
        let spans = extract_page_spans(backend, page_id)?;
        result.push((page_num as usize, spans));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Public API: font statistics
// ---------------------------------------------------------------------------

/// Quantise a font size into a histogram bucket.
fn bucket(size: f32) -> f32 {
    (size / FONT_SIZE_BUCKET).round() * FONT_SIZE_BUCKET
}

/// Build aggregate font-size statistics from all extracted spans.
///
/// The histogram counts *characters* (not spans) at each quantised font size.
/// `body_size` is the bucket with the most characters.
/// `heading_threshold` is `body_size + 1.5`.
pub fn build_font_statistics(all_spans: &[(usize, Vec<TextSpan>)]) -> FontStatistics {
    let mut histogram: HashMap<i32, usize> = HashMap::new();

    for (_page, spans) in all_spans {
        for span in spans {
            if span.font_size <= 0.0 {
                continue;
            }
            let key = (bucket(span.font_size) * 100.0).round() as i32;
            let char_count = span.text.chars().count();
            *histogram.entry(key).or_insert(0) += char_count;
        }
    }

    // Convert to Vec and sort descending by size (for heading-level assignment).
    let mut size_histogram: Vec<(f32, usize)> = histogram
        .into_iter()
        .map(|(k, v)| (k as f32 / 100.0, v))
        .collect();
    size_histogram.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Body size is the bucket with the highest character count.
    let body_size = size_histogram
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(size, _)| *size)
        .unwrap_or(12.0);

    let heading_threshold = body_size + 1.5;

    FontStatistics {
        body_size,
        heading_threshold,
        size_histogram,
    }
}

// ---------------------------------------------------------------------------
// Public API: span -> line grouping
// ---------------------------------------------------------------------------

/// Group a flat list of [`TextSpan`]s into [`TextLine`]s.
///
/// Spans whose Y coordinates are within [`Y_TOLERANCE`] points of each other
/// are placed on the same line.  Within a line, spans are sorted left-to-right
/// by X and spaces are inserted between spans that are not adjacent.
pub fn group_spans_into_lines(mut spans: Vec<TextSpan>) -> Vec<TextLine> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Sort by Y descending (top of page first), then X ascending.
    spans.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut lines: Vec<TextLine> = Vec::new();
    let mut current_spans: Vec<TextSpan> = vec![spans.remove(0)];
    let mut current_y = current_spans[0].y;

    for span in spans {
        if (span.y - current_y).abs() <= Y_TOLERANCE {
            current_spans.push(span);
        } else {
            lines.push(assemble_line(std::mem::take(&mut current_spans)));
            current_y = span.y;
            current_spans.push(span);
        }
    }

    if !current_spans.is_empty() {
        lines.push(assemble_line(current_spans));
    }

    lines
}

/// Build a [`TextLine`] from a set of spans known to share the same Y.
///
/// Spans are sorted left-to-right.  When a gap between consecutive spans
/// exceeds [`MIN_WORD_GAP`] points and neither boundary character is a
/// spaceless-script character, an inter-word space is added.
fn assemble_line(mut spans: Vec<TextSpan>) -> TextLine {
    // Sort left-to-right by X.
    spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    // Merge spans that overlap or are very close, inserting spaces as needed.
    let mut merged: Vec<TextSpan> = Vec::with_capacity(spans.len());

    for span in spans {
        if let Some(prev) = merged.last_mut() {
            let prev_end = prev.x + prev.width;
            let gap = span.x - prev_end;

            let same_font = prev.font_name == span.font_name
                && (prev.font_size - span.font_size).abs() < FONT_SIZE_BUCKET
                && prev.is_bold == span.is_bold
                && prev.is_italic == span.is_italic;

            if same_font && gap < MIN_WORD_GAP && gap > -prev.font_size {
                // Adjacent or overlapping -- concatenate directly.
                prev.text.push_str(&span.text);
                prev.width = (span.x + span.width) - prev.x;
                continue;
            }

            if same_font && gap >= MIN_WORD_GAP && gap < prev.font_size * 2.0 {
                // Meaningful gap but still the same run -- insert a space
                // unless both boundary characters are from spaceless scripts.
                let needs_space = !boundary_is_spaceless(prev, &span);
                if needs_space {
                    prev.text.push(' ');
                }
                prev.text.push_str(&span.text);
                prev.width = (span.x + span.width) - prev.x;
                continue;
            }
        }

        merged.push(span);
    }

    // Compute line-level properties from the dominant span.
    let y = merged.first().map(|s| s.y).unwrap_or(0.0);
    let x = merged.first().map(|s| s.x).unwrap_or(0.0);
    let font_size = dominant_font_size(&merged);

    TextLine {
        spans: merged,
        y,
        x,
        font_size,
        is_heading: false,
        heading_level: 0,
    }
}

/// Returns the font size that covers the most characters in the spans.
fn dominant_font_size(spans: &[TextSpan]) -> f32 {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for s in spans {
        let key = (s.font_size * 100.0).round() as i32;
        *counts.entry(key).or_insert(0) += s.text.chars().count();
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k as f32 / 100.0)
        .unwrap_or(0.0)
}

/// Check whether the boundary between two adjacent spans is between
/// spaceless-script characters (no space needed).
fn boundary_is_spaceless(prev: &TextSpan, next: &TextSpan) -> bool {
    let last_char = prev.text.chars().next_back();
    let first_char = next.text.chars().next();
    match (last_char, first_char) {
        (Some(l), Some(f)) => is_spaceless_script_char(l) && is_spaceless_script_char(f),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Public API: heading detection
// ---------------------------------------------------------------------------

/// Classify lines as headings based on font-size statistics.
///
/// A line is a heading if:
/// 1. Its dominant font size exceeds `font_stats.heading_threshold`, AND
/// 2. It is not excessively long (heuristic: at most 200 characters).
///
/// Heading levels (1..=6) are assigned by ranking distinct heading sizes in
/// descending order.  The largest heading size maps to level 1.
///
/// Lines that are bold but at body size are *not* promoted to headings -- only
/// font size drives the classification.
pub fn detect_headings(lines: &mut [TextLine], font_stats: &FontStatistics) {
    // Collect distinct heading sizes present in the lines.
    let mut heading_sizes: Vec<f32> = Vec::new();
    for line in lines.iter() {
        if line.font_size > font_stats.heading_threshold && line_char_count(line) <= 200 {
            let b = bucket(line.font_size);
            if !heading_sizes
                .iter()
                .any(|&s| (s - b).abs() < FONT_SIZE_BUCKET)
            {
                heading_sizes.push(b);
            }
        }
    }

    // Sort descending so largest = level 1.
    heading_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    // Clamp to 6 levels maximum.
    heading_sizes.truncate(6);

    // Assign levels.
    for line in lines.iter_mut() {
        if line.font_size > font_stats.heading_threshold && line_char_count(line) <= 200 {
            let b = bucket(line.font_size);
            if let Some(pos) = heading_sizes
                .iter()
                .position(|&s| (s - b).abs() < FONT_SIZE_BUCKET)
            {
                line.is_heading = true;
                line.heading_level = (pos as u8) + 1; // 1-based
            }
        }
    }
}

/// Total character count across all spans in a line.
fn line_char_count(line: &TextLine) -> usize {
    line.spans.iter().map(|s| s.text.chars().count()).sum()
}

// ---------------------------------------------------------------------------
// Public API: line -> block grouping
// ---------------------------------------------------------------------------

/// Detect whether a line looks like a list item (starts with a bullet, dash,
/// number+period, or similar marker).
fn is_list_item(line: &TextLine) -> bool {
    let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
    let trimmed = text.trim_start();

    if trimmed.is_empty() {
        return false;
    }

    // Bullet characters.
    let first = trimmed.chars().next().unwrap();
    if matches!(
        first,
        '\u{2022}' | '\u{2023}' | '\u{25E6}' | '\u{2043}' | '\u{2219}'
    ) {
        return true;
    }

    // Dash-style bullets.
    if trimmed.starts_with("- ") || trimmed.starts_with("-- ") || trimmed.starts_with("\u{2013} ") {
        return true;
    }

    // Numbered list: "1." "2)" "a." "a)" "(a)" "(1)" etc.
    let bytes = trimmed.as_bytes();

    // "(X)" style
    if bytes.first() == Some(&b'(') {
        if let Some(close) = trimmed.find(')') {
            if close <= 4 {
                let inner = &trimmed[1..close];
                if inner
                    .chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_alphabetic())
                {
                    return true;
                }
            }
        }
    }

    // "X." or "X)" style where X is digits or a single letter.
    if let Some(pos) = trimmed.find(['.', ')']) {
        if pos <= 3 {
            let prefix = &trimmed[..pos];
            if prefix.chars().all(|c| c.is_ascii_digit())
                || (prefix.len() == 1 && prefix.chars().all(|c| c.is_ascii_alphabetic()))
            {
                // Ensure there is a space after the marker (or it ends the text).
                if trimmed.get(pos + 1..pos + 2) == Some(" ") || pos + 1 == trimmed.len() {
                    return true;
                }
            }
        }
    }

    false
}

/// Determine the [`BlockType`] for a single line.
fn classify_line(line: &TextLine) -> BlockType {
    if line.is_heading {
        BlockType::Heading(line.heading_level)
    } else if is_list_item(line) {
        BlockType::ListItem
    } else {
        BlockType::Paragraph
    }
}

/// Group consecutive [`TextLine`]s into [`TextBlock`]s.
///
/// A new block starts when:
/// - The line type changes (heading vs paragraph vs list item).
/// - The vertical gap between consecutive lines exceeds
///   [`BLOCK_GAP_FACTOR`] times the font size.
/// - A heading line always starts its own single-line block.
pub fn group_lines_into_blocks(lines: Vec<TextLine>) -> Vec<TextBlock> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut current_lines: Vec<TextLine> = Vec::new();
    let mut current_type: Option<BlockType> = None;

    for line in lines {
        let line_type = classify_line(&line);

        // Headings always form their own block.
        if matches!(line_type, BlockType::Heading(_)) {
            // Flush any accumulated block.
            if !current_lines.is_empty() {
                let bt = current_type.take().unwrap_or(BlockType::Paragraph);
                blocks.push(TextBlock {
                    lines: std::mem::take(&mut current_lines),
                    block_type: bt,
                });
            }
            blocks.push(TextBlock {
                lines: vec![line],
                block_type: line_type,
            });
            current_type = None;
            continue;
        }

        // Check for vertical gap that would break the block.
        let gap_break = if let Some(prev) = current_lines.last() {
            let gap = (prev.y - line.y).abs();
            let threshold = prev.font_size * BLOCK_GAP_FACTOR;
            gap > threshold
        } else {
            false
        };

        // Check for block-type change.
        let type_change = current_type.as_ref().is_some_and(|ct| *ct != line_type);

        if (gap_break || type_change) && !current_lines.is_empty() {
            let bt = current_type.take().unwrap_or(BlockType::Paragraph);
            blocks.push(TextBlock {
                lines: std::mem::take(&mut current_lines),
                block_type: bt,
            });
        }

        current_type = Some(line_type);
        current_lines.push(line);
    }

    // Flush the last block.
    if !current_lines.is_empty() {
        let bt = current_type.unwrap_or(BlockType::Paragraph);
        blocks.push(TextBlock {
            lines: current_lines,
            block_type: bt,
        });
    }

    blocks
}

// ---------------------------------------------------------------------------
// Public API: full pipeline
// ---------------------------------------------------------------------------

/// Run the complete layout-analysis pipeline on pre-extracted page spans.
///
/// 1. Build document-wide font statistics.
/// 2. For each page: group spans into lines, detect headings, group into
///    blocks.
///
/// Returns `(page_number, blocks)` pairs.
pub fn analyze(
    pages: Vec<(usize, Vec<TextSpan>)>,
) -> Result<Vec<(usize, Vec<TextBlock>)>, PdfError> {
    let font_stats = build_font_statistics(&pages);

    let mut result: Vec<(usize, Vec<TextBlock>)> = Vec::with_capacity(pages.len());

    for (page_num, spans) in pages {
        let mut lines = group_spans_into_lines(spans);
        detect_headings(&mut lines, &font_stats);
        let blocks = group_lines_into_blocks(lines);
        result.push((page_num, blocks));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::backend::ContentOp;
    use super::*;

    // -- Helpers for building test data -----------------------------------

    fn make_span(text: &str, x: f32, y: f32, font_size: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len() as f32 * font_size * APPROX_CHAR_WIDTH_RATIO,
            font_size,
            font_name: "TestFont".to_string(),
            is_bold: false,
            is_italic: false,
        }
    }

    fn make_bold_span(text: &str, x: f32, y: f32, font_size: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len() as f32 * font_size * APPROX_CHAR_WIDTH_RATIO,
            font_size,
            font_name: "TestFont-Bold".to_string(),
            is_bold: true,
            is_italic: false,
        }
    }

    fn make_line(spans: Vec<TextSpan>, font_size: f32, y: f32) -> TextLine {
        let x = spans.first().map(|s| s.x).unwrap_or(0.0);
        TextLine {
            spans,
            y,
            x,
            font_size,
            is_heading: false,
            heading_level: 0,
        }
    }

    // =====================================================================
    // build_font_statistics
    // =====================================================================

    #[test]
    fn test_font_statistics_body_size() {
        // Many characters at 12pt, fewer at 18pt and 24pt.
        let pages = vec![(
            1,
            vec![
                make_span(&"a".repeat(500), 0.0, 700.0, 12.0),
                make_span(&"b".repeat(50), 0.0, 680.0, 18.0),
                make_span(&"c".repeat(10), 0.0, 660.0, 24.0),
            ],
        )];

        let stats = build_font_statistics(&pages);
        assert!(
            (stats.body_size - 12.0).abs() < FONT_SIZE_BUCKET,
            "body_size should be ~12.0, got {}",
            stats.body_size
        );
        assert!(
            (stats.heading_threshold - 13.5).abs() < FONT_SIZE_BUCKET,
            "heading_threshold should be ~13.5, got {}",
            stats.heading_threshold
        );
    }

    #[test]
    fn test_font_statistics_empty_input() {
        let pages: Vec<(usize, Vec<TextSpan>)> = vec![];
        let stats = build_font_statistics(&pages);
        // Falls back to 12.0 when no spans are present.
        assert!((stats.body_size - 12.0).abs() < 0.01);
    }

    #[test]
    fn test_font_statistics_histogram_sorted_descending() {
        let pages = vec![(
            1,
            vec![
                make_span("small", 0.0, 700.0, 10.0),
                make_span("medium", 0.0, 680.0, 14.0),
                make_span("large", 0.0, 660.0, 20.0),
            ],
        )];

        let stats = build_font_statistics(&pages);
        for w in stats.size_histogram.windows(2) {
            assert!(
                w[0].0 >= w[1].0,
                "histogram not sorted descending: {:?}",
                stats.size_histogram
            );
        }
    }

    #[test]
    fn test_font_statistics_ignores_zero_size() {
        let pages = vec![(
            1,
            vec![
                make_span("visible", 0.0, 700.0, 12.0),
                make_span("invisible", 0.0, 680.0, 0.0),
            ],
        )];

        let stats = build_font_statistics(&pages);
        // Only the 12pt bucket should appear.
        assert_eq!(stats.size_histogram.len(), 1);
    }

    // =====================================================================
    // group_spans_into_lines
    // =====================================================================

    #[test]
    fn test_group_spans_same_y() {
        let spans = vec![
            make_span("Hello", 0.0, 700.0, 12.0),
            make_span("World", 40.0, 700.0, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(lines.len(), 1, "both spans should be on the same line");
        assert!((lines[0].y - 700.0).abs() < Y_TOLERANCE);
    }

    #[test]
    fn test_group_spans_different_y() {
        let spans = vec![
            make_span("Line 1", 0.0, 700.0, 12.0),
            make_span("Line 2", 0.0, 680.0, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(
            lines.len(),
            2,
            "different Y values should produce two lines"
        );
    }

    #[test]
    fn test_group_spans_within_tolerance() {
        let spans = vec![
            make_span("A", 0.0, 700.0, 12.0),
            make_span("B", 50.0, 700.5, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(lines.len(), 1, "Y values within tolerance should merge");
    }

    #[test]
    fn test_group_spans_outside_tolerance() {
        let spans = vec![
            make_span("A", 0.0, 700.0, 12.0),
            make_span("B", 50.0, 698.0, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(
            lines.len(),
            2,
            "Y values outside tolerance should produce two lines"
        );
    }

    #[test]
    fn test_group_spans_empty() {
        let lines = group_spans_into_lines(vec![]);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_group_spans_sorted_by_x() {
        let spans = vec![
            make_span("World", 100.0, 700.0, 12.0),
            make_span("Hello", 0.0, 700.0, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].x < 50.0, "line should start at the leftmost span");
    }

    #[test]
    fn test_group_spans_multiple_lines_ordered_top_to_bottom() {
        let spans = vec![
            make_span("Bottom", 0.0, 600.0, 12.0),
            make_span("Middle", 0.0, 650.0, 12.0),
            make_span("Top", 0.0, 700.0, 12.0),
        ];

        let lines = group_spans_into_lines(spans);
        assert_eq!(lines.len(), 3);
        // Lines should be sorted Y descending (top of page first).
        assert!(lines[0].y > lines[1].y);
        assert!(lines[1].y > lines[2].y);
    }

    // =====================================================================
    // detect_headings
    // =====================================================================

    #[test]
    fn test_detect_headings_basic() {
        let font_stats = FontStatistics {
            body_size: 12.0,
            heading_threshold: 13.5,
            size_histogram: vec![(24.0, 10), (18.0, 20), (12.0, 500)],
        };

        let mut lines = vec![
            make_line(vec![make_span("Title", 0.0, 750.0, 24.0)], 24.0, 750.0),
            make_line(vec![make_span("Subtitle", 0.0, 720.0, 18.0)], 18.0, 720.0),
            make_line(
                vec![make_span("Body text here.", 0.0, 700.0, 12.0)],
                12.0,
                700.0,
            ),
        ];

        detect_headings(&mut lines, &font_stats);

        assert!(lines[0].is_heading, "24pt line should be heading");
        assert_eq!(lines[0].heading_level, 1, "24pt should be H1");

        assert!(lines[1].is_heading, "18pt line should be heading");
        assert_eq!(lines[1].heading_level, 2, "18pt should be H2");

        assert!(!lines[2].is_heading, "12pt line should not be heading");
        assert_eq!(lines[2].heading_level, 0);
    }

    #[test]
    fn test_detect_headings_long_line_excluded() {
        let font_stats = FontStatistics {
            body_size: 12.0,
            heading_threshold: 13.5,
            size_histogram: vec![(18.0, 20), (12.0, 500)],
        };

        let long_text = "A".repeat(250);
        let mut lines = vec![make_line(
            vec![make_span(&long_text, 0.0, 750.0, 18.0)],
            18.0,
            750.0,
        )];

        detect_headings(&mut lines, &font_stats);

        assert!(
            !lines[0].is_heading,
            "very long large-font line should not be heading"
        );
    }

    #[test]
    fn test_detect_headings_multiple_same_size() {
        let font_stats = FontStatistics {
            body_size: 12.0,
            heading_threshold: 13.5,
            size_histogram: vec![(18.0, 40), (12.0, 500)],
        };

        let mut lines = vec![
            make_line(vec![make_span("Section A", 0.0, 750.0, 18.0)], 18.0, 750.0),
            make_line(vec![make_span("Section B", 0.0, 700.0, 18.0)], 18.0, 700.0),
        ];

        detect_headings(&mut lines, &font_stats);

        assert!(lines[0].is_heading);
        assert!(lines[1].is_heading);
        assert_eq!(
            lines[0].heading_level, lines[1].heading_level,
            "same size should yield same level"
        );
    }

    #[test]
    fn test_detect_headings_up_to_six_levels() {
        let font_stats = FontStatistics {
            body_size: 10.0,
            heading_threshold: 11.5,
            size_histogram: vec![],
        };

        // Create 7 distinct heading sizes; only 6 should get levels.
        let sizes = [30.0, 26.0, 22.0, 18.0, 16.0, 14.0, 12.0];
        let mut lines: Vec<TextLine> = sizes
            .iter()
            .enumerate()
            .map(|(i, &sz)| {
                make_line(
                    vec![make_span(
                        &format!("H{}", i),
                        0.0,
                        700.0 - i as f32 * 20.0,
                        sz,
                    )],
                    sz,
                    700.0 - i as f32 * 20.0,
                )
            })
            .collect();

        detect_headings(&mut lines, &font_stats);

        // First 6 should be headings with levels 1..=6.
        for (i, line) in lines[..6].iter().enumerate() {
            assert!(line.is_heading, "line {} should be heading", i);
            assert_eq!(line.heading_level, (i as u8) + 1);
        }
        // The 7th (size 12.0) should not get a level because we truncate to 6.
        assert!(!lines[6].is_heading);
    }

    // =====================================================================
    // is_spaceless_script_char
    // =====================================================================

    #[test]
    fn test_spaceless_cjk_ideograph() {
        assert!(is_spaceless_script_char('\u{4E00}')); // first CJK char
        assert!(is_spaceless_script_char('\u{9FFF}')); // last in basic block
    }

    #[test]
    fn test_spaceless_hiragana() {
        assert!(is_spaceless_script_char('\u{3042}')); // hiragana 'a'
    }

    #[test]
    fn test_spaceless_katakana() {
        assert!(is_spaceless_script_char('\u{30A2}')); // katakana 'a'
    }

    #[test]
    fn test_spaceless_hangul() {
        assert!(is_spaceless_script_char('\u{AC00}')); // first Hangul syllable
    }

    #[test]
    fn test_spaceless_thai() {
        assert!(is_spaceless_script_char('\u{0E01}')); // Thai ko kai
    }

    #[test]
    fn test_spaceless_latin_returns_false() {
        assert!(!is_spaceless_script_char('A'));
        assert!(!is_spaceless_script_char('z'));
        assert!(!is_spaceless_script_char(' '));
        assert!(!is_spaceless_script_char('1'));
    }

    #[test]
    fn test_spaceless_extension_b() {
        assert!(is_spaceless_script_char('\u{20000}')); // CJK Extension B start
    }

    // =====================================================================
    // group_lines_into_blocks
    // =====================================================================

    #[test]
    fn test_group_lines_heading_starts_new_block() {
        let lines = vec![
            TextLine {
                spans: vec![make_span("Title", 0.0, 750.0, 24.0)],
                y: 750.0,
                x: 0.0,
                font_size: 24.0,
                is_heading: true,
                heading_level: 1,
            },
            TextLine {
                spans: vec![make_span("Paragraph.", 0.0, 730.0, 12.0)],
                y: 730.0,
                x: 0.0,
                font_size: 12.0,
                is_heading: false,
                heading_level: 0,
            },
            TextLine {
                spans: vec![make_span("More text.", 0.0, 718.0, 12.0)],
                y: 718.0,
                x: 0.0,
                font_size: 12.0,
                is_heading: false,
                heading_level: 0,
            },
        ];

        let blocks = group_lines_into_blocks(lines);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, BlockType::Heading(1));
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[1].block_type, BlockType::Paragraph);
        assert_eq!(blocks[1].lines.len(), 2);
    }

    #[test]
    fn test_group_lines_empty() {
        let blocks = group_lines_into_blocks(vec![]);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_group_lines_list_items_grouped() {
        let lines = vec![
            make_line(vec![make_span("- item 1", 0.0, 700.0, 12.0)], 12.0, 700.0),
            make_line(vec![make_span("- item 2", 0.0, 688.0, 12.0)], 12.0, 688.0),
        ];

        let blocks = group_lines_into_blocks(lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, BlockType::ListItem);
        assert_eq!(blocks[0].lines.len(), 2);
    }

    #[test]
    fn test_group_lines_gap_breaks_block() {
        // Two paragraph lines separated by a large gap.
        let lines = vec![
            make_line(
                vec![make_span("First paragraph.", 0.0, 700.0, 12.0)],
                12.0,
                700.0,
            ),
            make_line(
                vec![make_span("Second paragraph.", 0.0, 650.0, 12.0)],
                12.0,
                650.0,
            ),
        ];

        let blocks = group_lines_into_blocks(lines);
        // Gap of 50pt > 12.0 * 1.4 = 16.8, should split.
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, BlockType::Paragraph);
        assert_eq!(blocks[1].block_type, BlockType::Paragraph);
    }

    #[test]
    fn test_group_lines_type_change_breaks_block() {
        let lines = vec![
            make_line(
                vec![make_span("Normal text.", 0.0, 700.0, 12.0)],
                12.0,
                700.0,
            ),
            make_line(
                vec![make_span("- list item", 0.0, 688.0, 12.0)],
                12.0,
                688.0,
            ),
        ];

        let blocks = group_lines_into_blocks(lines);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, BlockType::Paragraph);
        assert_eq!(blocks[1].block_type, BlockType::ListItem);
    }

    #[test]
    fn test_group_lines_consecutive_headings() {
        let lines = vec![
            TextLine {
                spans: vec![make_span("Title", 0.0, 750.0, 24.0)],
                y: 750.0,
                x: 0.0,
                font_size: 24.0,
                is_heading: true,
                heading_level: 1,
            },
            TextLine {
                spans: vec![make_span("Subtitle", 0.0, 720.0, 18.0)],
                y: 720.0,
                x: 0.0,
                font_size: 18.0,
                is_heading: true,
                heading_level: 2,
            },
        ];

        let blocks = group_lines_into_blocks(lines);
        // Each heading should be its own block.
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, BlockType::Heading(1));
        assert_eq!(blocks[1].block_type, BlockType::Heading(2));
    }

    // =====================================================================
    // is_list_item
    // =====================================================================

    #[test]
    fn test_list_item_bullet() {
        let line = make_line(
            vec![make_span("\u{2022} Item one", 0.0, 700.0, 12.0)],
            12.0,
            700.0,
        );
        assert!(is_list_item(&line));
    }

    #[test]
    fn test_list_item_dash() {
        let line = make_line(
            vec![make_span("- Dashed item", 0.0, 700.0, 12.0)],
            12.0,
            700.0,
        );
        assert!(is_list_item(&line));
    }

    #[test]
    fn test_list_item_numbered() {
        let line = make_line(
            vec![make_span("1. First item", 0.0, 700.0, 12.0)],
            12.0,
            700.0,
        );
        assert!(is_list_item(&line));
    }

    #[test]
    fn test_list_item_paren_style() {
        let line = make_line(
            vec![make_span("(a) Sub-item", 0.0, 700.0, 12.0)],
            12.0,
            700.0,
        );
        assert!(is_list_item(&line));
    }

    #[test]
    fn test_not_list_item() {
        let line = make_line(
            vec![make_span("Regular paragraph text.", 0.0, 700.0, 12.0)],
            12.0,
            700.0,
        );
        assert!(!is_list_item(&line));
    }

    // =====================================================================
    // analyze (full pipeline)
    // =====================================================================

    #[test]
    fn test_analyze_full_pipeline() {
        let pages = vec![(
            1,
            vec![
                make_span("Document Title", 72.0, 750.0, 24.0),
                make_span(
                    "This is body text that goes on for a while.",
                    72.0,
                    700.0,
                    12.0,
                ),
                make_span("More body text in a second line.", 72.0, 688.0, 12.0),
                make_bold_span("Section Heading", 72.0, 650.0, 18.0),
                make_span("Content under section.", 72.0, 630.0, 12.0),
            ],
        )];

        let result = analyze(pages).unwrap();
        assert_eq!(result.len(), 1);

        let (page, blocks) = &result[0];
        assert_eq!(*page, 1);
        // Expect at least:
        //   - 1 heading block for "Document Title"
        //   - 1 paragraph block for body text
        //   - 1 heading block for "Section Heading"
        //   - 1 paragraph block for section content
        assert!(
            blocks.len() >= 3,
            "expected at least 3 blocks, got {}",
            blocks.len()
        );

        // First block should be a heading.
        assert!(
            matches!(blocks[0].block_type, BlockType::Heading(_)),
            "first block should be a heading, got {:?}",
            blocks[0].block_type
        );
    }

    #[test]
    fn test_analyze_empty_pages() {
        let pages: Vec<(usize, Vec<TextSpan>)> = vec![(1, vec![])];
        let result = analyze(pages).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].1.is_empty());
    }

    // =====================================================================
    // TextState internals
    // =====================================================================

    #[test]
    fn test_text_state_effective_font_size_identity() {
        let state = TextState {
            font_size: 12.0,
            text_matrix: IDENTITY_MATRIX,
            ..Default::default()
        };
        assert!((state.effective_font_size() - 12.0).abs() < 0.01);
    }

    #[test]
    fn test_text_state_effective_font_size_scaled() {
        let state = TextState {
            font_size: 12.0,
            text_matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            ..Default::default()
        };
        assert!((state.effective_font_size() - 24.0).abs() < 0.01);
    }

    #[test]
    fn test_text_state_translate_line() {
        let mut state = TextState::default();
        state.translate_line(100.0, -14.0);
        assert!((state.x() - 100.0).abs() < 0.01);
        assert!((state.y() - (-14.0)).abs() < 0.01);
    }

    #[test]
    fn test_text_state_translate_line_cumulative() {
        let mut state = TextState::default();
        state.translate_line(72.0, 700.0);
        state.translate_line(0.0, -14.0);
        assert!((state.x() - 72.0).abs() < 0.01);
        assert!((state.y() - 686.0).abs() < 0.01);
    }

    #[test]
    fn test_text_state_set_font_bold_italic() {
        let mut state = TextState::default();
        state.set_font(b"F1".to_vec(), "Helvetica-BoldItalic", 12.0);
        assert!(state.is_bold);
        assert!(state.is_italic);
        assert_eq!(state.font_name, "Helvetica-BoldItalic");
    }

    #[test]
    fn test_text_state_set_font_oblique() {
        let mut state = TextState::default();
        state.set_font(b"F2".to_vec(), "TimesNewRoman-Oblique", 10.0);
        assert!(!state.is_bold);
        assert!(state.is_italic);
    }

    #[test]
    fn test_text_state_set_font_regular() {
        let mut state = TextState::default();
        state.set_font(b"F3".to_vec(), "Courier", 11.0);
        assert!(!state.is_bold);
        assert!(!state.is_italic);
    }

    // =====================================================================
    // boundary_is_spaceless
    // =====================================================================

    #[test]
    fn test_boundary_spaceless_cjk() {
        let prev = TextSpan {
            text: "\u{4E00}".to_string(),
            x: 0.0,
            y: 0.0,
            width: 12.0,
            font_size: 12.0,
            font_name: "Font".to_string(),
            is_bold: false,
            is_italic: false,
        };
        let next = TextSpan {
            text: "\u{4E01}".to_string(),
            x: 12.0,
            y: 0.0,
            width: 12.0,
            font_size: 12.0,
            font_name: "Font".to_string(),
            is_bold: false,
            is_italic: false,
        };
        assert!(boundary_is_spaceless(&prev, &next));
    }

    #[test]
    fn test_boundary_not_spaceless_latin() {
        let prev = make_span("Hello", 0.0, 700.0, 12.0);
        let next = make_span("World", 40.0, 700.0, 12.0);
        assert!(!boundary_is_spaceless(&prev, &next));
    }

    // =====================================================================
    // bucket helper
    // =====================================================================

    #[test]
    fn test_bucket_rounds_correctly() {
        assert!((bucket(12.0) - 12.0).abs() < 0.01);
        assert!((bucket(12.2) - 12.0).abs() < 0.01);
        assert!((bucket(12.3) - 12.5).abs() < 0.01);
        assert!((bucket(12.7) - 12.5).abs() < 0.01);
        assert!((bucket(12.8) - 13.0).abs() < 0.01);
    }

    // =====================================================================
    // extract_page_spans with mock backend
    // =====================================================================

    /// A minimal mock backend for testing the state machine.
    struct MockBackend {
        page_ids: BTreeMap<u32, PageId>,
        fonts: Vec<BackendFontInfo>,
        /// Raw content bytes are unused; we store pre-decoded ops directly.
        ops: Vec<ContentOp>,
    }

    impl PdfBackend for MockBackend {
        fn pages(&self) -> BTreeMap<u32, PageId> {
            self.page_ids.clone()
        }

        fn page_fonts(&self, _page_id: PageId) -> Result<Vec<BackendFontInfo>, PdfError> {
            Ok(self.fonts.clone())
        }

        fn page_content(&self, _page_id: PageId) -> Result<Vec<u8>, PdfError> {
            // Return empty bytes; decode_content returns pre-stored ops.
            Ok(vec![])
        }

        fn decode_content(&self, _data: &[u8]) -> Result<Vec<ContentOp>, PdfError> {
            Ok(self.ops.clone())
        }

        fn decode_text(&self, _page: PageId, _font_name: &[u8], data: &[u8]) -> String {
            super::super::backend::decode_text_simple(data)
        }
    }

    fn make_op(operator: &str, operands: Vec<PdfValue>) -> ContentOp {
        ContentOp {
            operator: operator.to_string(),
            operands,
        }
    }

    fn helvetica_font() -> Vec<BackendFontInfo> {
        vec![BackendFontInfo {
            name: b"F1".to_vec(),
            base_font: Some("Helvetica".to_string()),
            subtype: None,
            encoding: None,
        }]
    }

    fn mock_page_ids(ids: &[PageId]) -> BTreeMap<u32, PageId> {
        ids.iter()
            .enumerate()
            .map(|(i, &id)| ((i as u32) + 1, id))
            .collect()
    }

    fn bt_op() -> ContentOp {
        make_op("BT", vec![])
    }

    fn et_op() -> ContentOp {
        make_op("ET", vec![])
    }

    fn tf_op(font: &[u8], size: f32) -> ContentOp {
        make_op(
            "Tf",
            vec![PdfValue::Name(font.to_vec()), PdfValue::Real(size)],
        )
    }

    fn tm_op(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> ContentOp {
        make_op(
            "Tm",
            vec![
                PdfValue::Real(a),
                PdfValue::Real(b),
                PdfValue::Real(c),
                PdfValue::Real(d),
                PdfValue::Real(tx),
                PdfValue::Real(ty),
            ],
        )
    }

    fn td_op(tx: f32, ty: f32) -> ContentOp {
        make_op("Td", vec![PdfValue::Real(tx), PdfValue::Real(ty)])
    }

    fn tj_op(text: &[u8]) -> ContentOp {
        make_op("Tj", vec![PdfValue::Str(text.to_vec())])
    }

    fn tj_array_op(elements: Vec<PdfValue>) -> ContentOp {
        make_op("TJ", vec![PdfValue::Array(elements)])
    }

    #[test]
    fn test_extract_simple_tj() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_op(b"Hello World"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Hello World");
        assert!((spans[0].x - 72.0).abs() < 0.01);
        assert!((spans[0].y - 700.0).abs() < 0.01);
        assert!((spans[0].font_size - 12.0).abs() < 0.01);
        assert!(!spans[0].is_bold);
        assert!(!spans[0].is_italic);
    }

    #[test]
    fn test_extract_bold_font() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: vec![BackendFontInfo {
                name: b"F2".to_vec(),
                base_font: Some("Helvetica-Bold".to_string()),
                subtype: None,
                encoding: None,
            }],
            ops: vec![
                bt_op(),
                tf_op(b"F2", 14.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 750.0),
                tj_op(b"Bold Title"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].is_bold);
        assert!(!spans[0].is_italic);
        assert_eq!(spans[0].font_name, "Helvetica-Bold");
    }

    #[test]
    fn test_extract_italic_oblique_font() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: vec![BackendFontInfo {
                name: b"F3".to_vec(),
                base_font: Some("Times-Oblique".to_string()),
                subtype: None,
                encoding: None,
            }],
            ops: vec![
                bt_op(),
                tf_op(b"F3", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_op(b"Italic text"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].is_italic);
        assert!(!spans[0].is_bold);
    }

    #[test]
    fn test_extract_tj_array() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_array_op(vec![
                    PdfValue::Str(b"Hel".to_vec()),
                    PdfValue::Integer(-10),
                    PdfValue::Str(b"lo".to_vec()),
                ]),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert!(
            spans[0].text.contains("Hello"),
            "expected 'Hello', got: {}",
            spans[0].text
        );
    }

    #[test]
    fn test_extract_tj_array_with_large_kerning_inserts_space() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_array_op(vec![
                    PdfValue::Str(b"Hello".to_vec()),
                    PdfValue::Integer(-500), // large gap
                    PdfValue::Str(b"World".to_vec()),
                ]),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert!(
            spans[0].text.contains("Hello World"),
            "expected space between words, got: '{}'",
            spans[0].text
        );
    }

    #[test]
    fn test_extract_td_positioning() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                td_op(72.0, 700.0),
                tj_op(b"First"),
                td_op(0.0, -14.0),
                tj_op(b"Second"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        assert!((spans[0].y - 700.0).abs() < 0.01);
        assert!(
            (spans[1].y - 686.0).abs() < 0.01,
            "second span y should be 700 - 14 = 686, got {}",
            spans[1].y
        );
    }

    #[test]
    fn test_extract_td_capital_sets_leading() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                make_op("TD", vec![PdfValue::Real(72.0), PdfValue::Real(-14.0)]),
                tj_op(b"Line 1"),
                // T* should use leading set by TD (-(-14) = 14).
                make_op("T*", vec![]),
                tj_op(b"Line 2"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        // First line at y = -14.0 (from identity origin).
        // Second line should be 14 points below: -14.0 - 14.0 = -28.0
        assert!(
            (spans[1].y - (-28.0)).abs() < 0.01,
            "expected y=-28, got {}",
            spans[1].y
        );
    }

    #[test]
    fn test_extract_tl_and_t_star() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                make_op("TL", vec![PdfValue::Real(14.0)]),
                td_op(72.0, 700.0),
                tj_op(b"Line 1"),
                make_op("T*", vec![]),
                tj_op(b"Line 2"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        assert!((spans[0].y - 700.0).abs() < 0.01);
        assert!(
            (spans[1].y - 686.0).abs() < 0.01,
            "expected y=686, got {}",
            spans[1].y
        );
    }

    #[test]
    fn test_extract_single_quote_operator() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                make_op("TL", vec![PdfValue::Real(14.0)]),
                td_op(72.0, 700.0),
                tj_op(b"Line 1"),
                // ' operator: T* then Tj
                make_op("'", vec![PdfValue::Str(b"Line 2".to_vec())]),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[1].text, "Line 2");
        assert!(
            (spans[1].y - 686.0).abs() < 0.01,
            "expected y=686 for ' operator, got {}",
            spans[1].y
        );
    }

    #[test]
    fn test_extract_double_quote_operator() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                make_op("TL", vec![PdfValue::Real(14.0)]),
                td_op(72.0, 700.0),
                tj_op(b"Line 1"),
                // " operator: set Tw, Tc, T*, Tj
                make_op(
                    "\"",
                    vec![
                        PdfValue::Real(0.0), // aw
                        PdfValue::Real(0.0), // ac
                        PdfValue::Str(b"Line 2".to_vec()),
                    ],
                ),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[1].text, "Line 2");
    }

    #[test]
    fn test_extract_bt_resets_matrix() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                td_op(72.0, 700.0),
                tj_op(b"First object"),
                et_op(),
                // Second text object: BT should reset the matrix.
                bt_op(),
                td_op(72.0, 600.0),
                tj_op(b"Second object"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 2);
        assert!((spans[1].y - 600.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_empty_string_ignored() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_op(b""),
                tj_op(b"Visible"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Visible");
    }

    #[test]
    fn test_extract_font_not_in_dict() {
        // Font key not in the fonts dict -- should still produce a span
        // with the key as the font name.
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0)]),
            fonts: vec![], // empty font dict
            ops: vec![
                bt_op(),
                tf_op(b"F99", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_op(b"Text"),
                et_op(),
            ],
        };

        let spans = extract_page_spans(&backend, (1, 0)).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].font_name, "F99");
    }

    // =====================================================================
    // extract_all_pages
    // =====================================================================

    #[test]
    fn test_extract_all_pages() {
        let backend = MockBackend {
            page_ids: mock_page_ids(&[(1, 0), (2, 0)]),
            fonts: helvetica_font(),
            ops: vec![
                bt_op(),
                tf_op(b"F1", 12.0),
                tm_op(1.0, 0.0, 0.0, 1.0, 72.0, 700.0),
                tj_op(b"Page text"),
                et_op(),
            ],
        };

        let result = extract_all_pages(&backend).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 1); // page numbers are 1-based
        assert_eq!(result[1].0, 2);
        assert!(!result[0].1.is_empty());
        assert!(!result[1].1.is_empty());
    }

    #[test]
    fn test_extract_all_pages_empty_document() {
        let backend = MockBackend {
            page_ids: BTreeMap::new(),
            fonts: vec![],
            ops: vec![],
        };

        let result = extract_all_pages(&backend).unwrap();
        assert!(result.is_empty());
    }

    // =====================================================================
    // TextLine::text() method
    // =====================================================================

    #[test]
    fn test_text_line_text_method() {
        let line = TextLine {
            spans: vec![
                make_span("Hello", 0.0, 700.0, 12.0),
                make_span("World", 40.0, 700.0, 12.0),
            ],
            ..Default::default()
        };
        assert_eq!(line.text(), "Hello World");
    }

    #[test]
    fn test_text_line_text_single_span() {
        let line = TextLine {
            spans: vec![make_span("Single", 0.0, 700.0, 12.0)],
            ..Default::default()
        };
        assert_eq!(line.text(), "Single");
    }

    #[test]
    fn test_text_line_text_empty_spans() {
        let line = TextLine::default();
        assert_eq!(line.text(), "");
    }
}
