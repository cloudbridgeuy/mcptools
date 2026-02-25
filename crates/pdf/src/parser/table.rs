use std::collections::BTreeMap;

use super::layout::{BlockType, TextBlock, TextLine, TextSpan};
use crate::types::ClassifiedBlock;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A detected table region within a page, including its spatial bounds,
/// column boundaries, and the row data that belongs to it.
#[derive(Debug, Clone)]
pub struct DetectedTable {
    pub top_y: f32,
    pub bottom_y: f32,
    pub left_x: f32,
    pub right_x: f32,
    /// Sorted X positions that mark the left edge of each column.
    pub columns: Vec<f32>,
    pub rows: Vec<TableRowData>,
}

/// A single row inside a detected table, carrying the Y coordinate and the
/// text spans that belong to this row.
#[derive(Debug, Clone)]
pub struct TableRowData {
    pub y: f32,
    pub spans: Vec<TextSpan>,
}

/// Tuning knobs for the table detection heuristic.
#[derive(Debug, Clone)]
pub struct TableDetectorConfig {
    /// Minimum number of rows required for a region to qualify as a table.
    pub min_rows: usize,
    /// Minimum number of columns required.
    pub min_columns: usize,
    /// Maximum number of columns allowed (guards against noise).
    pub max_columns: usize,
    /// Factor applied to the median font size to derive Y-tolerance when
    /// grouping spans into rows.  `y_tolerance = median_font_size * factor`.
    pub y_tolerance_factor: f32,
    /// Fraction of rows that must have spans aligning with a candidate column
    /// position for that position to be accepted as a column boundary.
    pub min_alignment_ratio: f32,
    /// Minimum horizontal gap (in PDF points) between two adjacent column
    /// boundaries.
    pub min_column_gap: f32,
}

impl Default for TableDetectorConfig {
    fn default() -> Self {
        Self {
            min_rows: 2,
            min_columns: 2,
            max_columns: 20,
            y_tolerance_factor: 0.3,
            min_alignment_ratio: 0.5,
            min_column_gap: 10.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point: classify_blocks
// ---------------------------------------------------------------------------

/// Convert the per-page `TextBlock` output from layout analysis into a flat
/// list of [`ClassifiedBlock`] values that the tree builder can consume.
///
/// * `BlockType::Heading(level)` produces `ClassifiedBlock::Heading`.
/// * `BlockType::Table` attempts structural table detection on the block's
///   spans; if detection succeeds the first detected table is used, otherwise
///   the block falls back to a paragraph.
/// * `BlockType::Paragraph` and `BlockType::ListItem` produce
///   `ClassifiedBlock::Paragraph`.
pub fn classify_blocks(pages: Vec<(usize, Vec<TextBlock>)>) -> Vec<ClassifiedBlock> {
    let config = TableDetectorConfig::default();
    let mut result: Vec<ClassifiedBlock> = Vec::new();

    for (page, blocks) in pages {
        for block in blocks {
            match block.block_type {
                BlockType::Heading(level) => {
                    let title = concat_block_text(&block.lines);
                    result.push(ClassifiedBlock::Heading { level, title, page });
                }
                BlockType::Table => {
                    let spans = collect_spans(&block.lines);
                    let tables = detect_tables(&spans, &config);
                    if let Some(table) = tables.into_iter().next() {
                        let (headers, rows) = table_to_grid(&table);
                        result.push(ClassifiedBlock::Table {
                            headers,
                            rows,
                            page,
                        });
                    } else {
                        // Detection did not find a valid table; fall back to
                        // paragraph.
                        let text = concat_block_text(&block.lines);
                        result.push(ClassifiedBlock::Paragraph { text, page });
                    }
                }
                BlockType::Paragraph | BlockType::ListItem => {
                    let text = concat_block_text(&block.lines);
                    result.push(ClassifiedBlock::Paragraph { text, page });
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Table detection pipeline
// ---------------------------------------------------------------------------

/// Detect table regions from a set of raw text spans.
///
/// The algorithm works as follows:
/// 1. Compute Y-tolerance from the median font size of all spans.
/// 2. Group spans into rows by Y coordinate.
/// 3. Detect column boundaries that appear frequently across rows.
/// 4. If enough columns and rows satisfy the alignment ratio the region is
///    accepted as a table.
pub fn detect_tables(spans: &[TextSpan], config: &TableDetectorConfig) -> Vec<DetectedTable> {
    if spans.is_empty() {
        return Vec::new();
    }

    let y_tolerance = compute_y_tolerance(spans, config.y_tolerance_factor);
    let rows = group_into_rows(spans, y_tolerance);

    if rows.len() < config.min_rows {
        return Vec::new();
    }

    let columns = detect_columns(&rows, config);

    if columns.len() < config.min_columns || columns.len() > config.max_columns {
        return Vec::new();
    }

    // Verify alignment ratio: count how many rows have at least one span
    // aligning with each detected column.
    let aligned_rows = rows
        .iter()
        .filter(|row| {
            let aligned_cols = columns
                .iter()
                .filter(|&&col_x| {
                    row.spans
                        .iter()
                        .any(|s| (s.x - col_x).abs() < config.min_column_gap)
                })
                .count();
            // Row is considered aligned if it matches at least half the columns.
            aligned_cols >= columns.len().div_ceil(2)
        })
        .count();

    let ratio = aligned_rows as f32 / rows.len() as f32;
    if ratio < config.min_alignment_ratio {
        return Vec::new();
    }

    // Compute bounding box.
    let top_y = rows.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
    let bottom_y = rows.iter().map(|r| r.y).fold(f32::NEG_INFINITY, f32::max);
    let left_x = spans.iter().map(|s| s.x).fold(f32::INFINITY, f32::min);
    let right_x = spans
        .iter()
        .map(|s| s.x + s.width)
        .fold(f32::NEG_INFINITY, f32::max);

    vec![DetectedTable {
        top_y,
        bottom_y,
        left_x,
        right_x,
        columns,
        rows,
    }]
}

/// Group text spans into rows by their Y coordinate.
///
/// Two spans belong to the same row when their Y values differ by no more
/// than `y_tolerance`.  Rows are returned sorted top-to-bottom (ascending Y).
pub fn group_into_rows(spans: &[TextSpan], y_tolerance: f32) -> Vec<TableRowData> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Sort spans by Y first, then by X.
    let mut sorted: Vec<&TextSpan> = spans.iter().collect();
    sorted.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut rows: Vec<TableRowData> = Vec::new();
    let mut current_y = sorted[0].y;
    let mut current_spans: Vec<TextSpan> = vec![sorted[0].clone()];

    for span in sorted.iter().skip(1) {
        if (span.y - current_y).abs() <= y_tolerance {
            current_spans.push((*span).clone());
        } else {
            // Compute the average Y for the row.
            let avg_y = current_spans.iter().map(|s| s.y).sum::<f32>() / current_spans.len() as f32;
            // Sort spans within row by X.
            current_spans
                .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            rows.push(TableRowData {
                y: avg_y,
                spans: std::mem::take(&mut current_spans),
            });
            current_y = span.y;
            current_spans.push((*span).clone());
        }
    }

    // Flush the last row.
    if !current_spans.is_empty() {
        let avg_y = current_spans.iter().map(|s| s.y).sum::<f32>() / current_spans.len() as f32;
        current_spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        rows.push(TableRowData {
            y: avg_y,
            spans: current_spans,
        });
    }

    rows
}

/// Detect column boundaries from a set of table rows.
///
/// The algorithm buckets the X-start positions of all spans across all rows,
/// counts how many rows contain a span starting near each bucketed position,
/// and keeps only those positions that appear in at least
/// `config.min_alignment_ratio` of the rows.  Adjacent positions closer than
/// `config.min_column_gap` are merged (the one with higher frequency wins).
pub fn detect_columns(rows: &[TableRowData], config: &TableDetectorConfig) -> Vec<f32> {
    if rows.is_empty() {
        return Vec::new();
    }

    let total_rows = rows.len();

    // Bucket X positions: round to nearest integer to cluster nearby values.
    let mut x_freq: BTreeMap<i32, (f32, usize)> = BTreeMap::new();
    for row in rows {
        // Track which buckets this row contributes to (one vote per bucket per row).
        let mut seen_buckets: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for span in &row.spans {
            let bucket = span.x.round() as i32;
            if seen_buckets.insert(bucket) {
                let entry = x_freq.entry(bucket).or_insert((0.0, 0));
                entry.0 += span.x; // accumulate for averaging
                entry.1 += 1;
            }
        }
    }

    // Keep only buckets that meet the minimum alignment ratio.
    let min_count = (total_rows as f32 * config.min_alignment_ratio).ceil() as usize;
    let mut candidates: Vec<(f32, usize)> = x_freq
        .values()
        .filter(|(_, count)| *count >= min_count)
        .map(|(sum, count)| (sum / *count as f32, *count))
        .collect();

    // Sort candidates by X position.
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Merge candidates that are closer than min_column_gap, keeping the one
    // with the higher frequency.
    let mut columns: Vec<f32> = Vec::new();
    for &(x, _count) in &candidates {
        if let Some(&last) = columns.last() {
            if (x - last).abs() < config.min_column_gap {
                continue;
            }
        }
        columns.push(x);
    }

    columns
}

// ---------------------------------------------------------------------------
// Bullet / number marker helpers
// ---------------------------------------------------------------------------

/// Return `true` when `text` looks like a bullet marker.
///
/// Recognised patterns: single-character bullets such as `-`, `*`,
/// `\u{2022}` (bullet), `\u{2023}` (triangular bullet), `\u{25E6}` (white
/// bullet), `\u{2043}` (hyphen bullet), `\u{25AA}` (small black square),
/// and `\u{25CB}` (white circle).
pub fn is_bullet_marker(text: &str) -> bool {
    let trimmed = text.trim();
    matches!(
        trimmed,
        "-" | "*"
            | "\u{2022}"
            | "\u{2023}"
            | "\u{25E6}"
            | "\u{2043}"
            | "\u{25AA}"
            | "\u{25CB}"
            | "\u{25CF}"
            | "\u{2013}"
            | "\u{2014}"
            | "\u{00BB}"
            | "\u{203A}"
    )
}

/// Return `true` when `text` matches a numbered-list marker such as
/// `1.`, `2)`, `a.`, `iv)`, `(3)`, etc.
pub fn is_number_marker(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Patterns:
    //   digits followed by . or )        e.g. "1." "12)" "3."
    //   lower/upper letter followed by ./) e.g. "a." "B)"
    //   roman numerals followed by ./)    e.g. "ii." "iv)" "III."
    //   parenthesised number             e.g. "(1)" "(a)"

    // Use a simple regex-free approach.

    // Strip optional leading '(' and trailing ')'.
    let (inner, had_parens) = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        (&trimmed[1..trimmed.len() - 1], true)
    } else {
        (trimmed, false)
    };

    if had_parens {
        // The inner part should be digits or a single letter or roman numerals.
        return is_digit_sequence(inner) || is_single_letter(inner) || is_roman_numeral(inner);
    }

    // Must end with '.' or ')'.
    let last = trimmed.as_bytes()[trimmed.len() - 1];
    if last != b'.' && last != b')' {
        return false;
    }

    let body = &trimmed[..trimmed.len() - 1];
    is_digit_sequence(body) || is_single_letter(body) || is_roman_numeral(body)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Concatenate all lines of a block into a single string.
fn concat_block_text(lines: &[TextLine]) -> String {
    lines
        .iter()
        .map(|line| line.text())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Flatten all spans out of a set of text lines.
fn collect_spans(lines: &[TextLine]) -> Vec<TextSpan> {
    lines.iter().flat_map(|l| l.spans.iter().cloned()).collect()
}

/// Compute the Y-tolerance used for row grouping.
///
/// Uses the median font size of all spans multiplied by the given factor.
fn compute_y_tolerance(spans: &[TextSpan], factor: f32) -> f32 {
    if spans.is_empty() {
        return 1.0;
    }
    let mut sizes: Vec<f32> = spans.iter().map(|s| s.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sizes[sizes.len() / 2];
    (median * factor).max(1.0)
}

/// Convert a `DetectedTable` into a `(headers, rows)` tuple of string
/// vectors suitable for `ClassifiedBlock::Table`.
///
/// The first row is treated as the header row.  Each cell is the
/// concatenation of all spans whose X position falls closest to a column
/// boundary.
fn table_to_grid(table: &DetectedTable) -> (Vec<String>, Vec<Vec<String>>) {
    let num_cols = table.columns.len();
    if num_cols == 0 || table.rows.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut all_rows: Vec<Vec<String>> = Vec::new();

    for row in &table.rows {
        let mut cells: Vec<String> = vec![String::new(); num_cols];
        for span in &row.spans {
            let col_idx = assign_column(span.x, &table.columns);
            if !cells[col_idx].is_empty() {
                cells[col_idx].push(' ');
            }
            cells[col_idx].push_str(span.text.trim());
        }
        all_rows.push(cells);
    }

    if all_rows.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let headers = all_rows.remove(0);
    (headers, all_rows)
}

/// Find the column index whose boundary X is closest to the given span X.
fn assign_column(x: f32, columns: &[f32]) -> usize {
    columns
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (x - **a)
                .abs()
                .partial_cmp(&(x - **b).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn is_digit_sequence(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn is_single_letter(s: &str) -> bool {
    s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

fn is_roman_numeral(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    lower
        .chars()
        .all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers to build test data ----------------------------------------

    fn span(text: &str, x: f32, y: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len() as f32 * 5.0,
            font_size: 10.0,
            font_name: "TestFont".to_string(),
            is_bold: false,
            is_italic: false,
        }
    }

    fn span_with_size(text: &str, x: f32, y: f32, font_size: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len() as f32 * 5.0,
            font_size,
            font_name: "TestFont".to_string(),
            is_bold: false,
            is_italic: false,
        }
    }

    fn text_line(spans: Vec<TextSpan>) -> TextLine {
        TextLine {
            spans,
            ..Default::default()
        }
    }

    fn text_block(lines: Vec<TextLine>, block_type: BlockType) -> TextBlock {
        TextBlock { lines, block_type }
    }

    // -- is_bullet_marker --------------------------------------------------

    #[test]
    fn bullet_marker_recognises_dash() {
        assert!(is_bullet_marker("-"));
        assert!(is_bullet_marker("  -  "));
    }

    #[test]
    fn bullet_marker_recognises_asterisk() {
        assert!(is_bullet_marker("*"));
    }

    #[test]
    fn bullet_marker_recognises_unicode_bullet() {
        assert!(is_bullet_marker("\u{2022}"));
        assert!(is_bullet_marker("\u{25CF}"));
    }

    #[test]
    fn bullet_marker_rejects_text() {
        assert!(!is_bullet_marker("hello"));
        assert!(!is_bullet_marker("--"));
        assert!(!is_bullet_marker(""));
    }

    // -- is_number_marker --------------------------------------------------

    #[test]
    fn number_marker_recognises_digit_dot() {
        assert!(is_number_marker("1."));
        assert!(is_number_marker("12."));
        assert!(is_number_marker("99."));
    }

    #[test]
    fn number_marker_recognises_digit_paren() {
        assert!(is_number_marker("1)"));
        assert!(is_number_marker("3)"));
    }

    #[test]
    fn number_marker_recognises_letter() {
        assert!(is_number_marker("a."));
        assert!(is_number_marker("B)"));
    }

    #[test]
    fn number_marker_recognises_roman() {
        assert!(is_number_marker("ii."));
        assert!(is_number_marker("iv)"));
        assert!(is_number_marker("III."));
    }

    #[test]
    fn number_marker_recognises_parenthesised() {
        assert!(is_number_marker("(1)"));
        assert!(is_number_marker("(a)"));
        assert!(is_number_marker("(iv)"));
    }

    #[test]
    fn number_marker_rejects_plain_text() {
        assert!(!is_number_marker("hello"));
        assert!(!is_number_marker(""));
        assert!(!is_number_marker("1"));
        assert!(!is_number_marker("abc."));
    }

    // -- group_into_rows ---------------------------------------------------

    #[test]
    fn group_into_rows_empty() {
        let rows = group_into_rows(&[], 2.0);
        assert!(rows.is_empty());
    }

    #[test]
    fn group_into_rows_single_row() {
        let spans = vec![span("A", 10.0, 100.0), span("B", 60.0, 101.0)];
        let rows = group_into_rows(&spans, 2.0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].spans.len(), 2);
        assert_eq!(rows[0].spans[0].text, "A");
        assert_eq!(rows[0].spans[1].text, "B");
    }

    #[test]
    fn group_into_rows_multiple_rows() {
        let spans = vec![
            span("A", 10.0, 100.0),
            span("B", 60.0, 100.5),
            span("C", 10.0, 120.0),
            span("D", 60.0, 120.3),
            span("E", 10.0, 140.0),
        ];
        let rows = group_into_rows(&spans, 2.0);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].spans.len(), 2); // A, B
        assert_eq!(rows[1].spans.len(), 2); // C, D
        assert_eq!(rows[2].spans.len(), 1); // E
    }

    #[test]
    fn group_into_rows_sorts_spans_by_x() {
        // Spans provided in reverse X order within the same row.
        let spans = vec![span("B", 60.0, 100.0), span("A", 10.0, 100.0)];
        let rows = group_into_rows(&spans, 2.0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].spans[0].text, "A");
        assert_eq!(rows[0].spans[1].text, "B");
    }

    // -- detect_columns ----------------------------------------------------

    #[test]
    fn detect_columns_finds_aligned_positions() {
        // Three rows, each with spans starting at x=10 and x=60.
        let rows = vec![
            TableRowData {
                y: 100.0,
                spans: vec![span("A", 10.0, 100.0), span("B", 60.0, 100.0)],
            },
            TableRowData {
                y: 120.0,
                spans: vec![span("C", 10.0, 120.0), span("D", 60.0, 120.0)],
            },
            TableRowData {
                y: 140.0,
                spans: vec![span("E", 10.0, 140.0), span("F", 60.0, 140.0)],
            },
        ];

        let config = TableDetectorConfig::default();
        let cols = detect_columns(&rows, &config);
        assert_eq!(cols.len(), 2);
        assert!((cols[0] - 10.0).abs() < 1.0);
        assert!((cols[1] - 60.0).abs() < 1.0);
    }

    #[test]
    fn detect_columns_empty_rows() {
        let config = TableDetectorConfig::default();
        let cols = detect_columns(&[], &config);
        assert!(cols.is_empty());
    }

    #[test]
    fn detect_columns_merges_nearby_positions() {
        // Spans at x=10, x=11 (should merge), x=60, x=61 (should merge).
        let rows = vec![
            TableRowData {
                y: 100.0,
                spans: vec![span("A", 10.0, 100.0), span("B", 60.0, 100.0)],
            },
            TableRowData {
                y: 120.0,
                spans: vec![span("C", 11.0, 120.0), span("D", 61.0, 120.0)],
            },
            TableRowData {
                y: 140.0,
                spans: vec![span("E", 10.0, 140.0), span("F", 60.0, 140.0)],
            },
        ];

        let config = TableDetectorConfig::default();
        let cols = detect_columns(&rows, &config);
        assert_eq!(cols.len(), 2);
    }

    #[test]
    fn detect_columns_respects_min_alignment_ratio() {
        // Two rows agree on x=10,60 but a third row has only x=30.
        // With 3 rows and min_alignment_ratio=0.5, x=30 appears in 1/3 < 0.5 rows.
        let rows = vec![
            TableRowData {
                y: 100.0,
                spans: vec![span("A", 10.0, 100.0), span("B", 60.0, 100.0)],
            },
            TableRowData {
                y: 120.0,
                spans: vec![span("C", 10.0, 120.0), span("D", 60.0, 120.0)],
            },
            TableRowData {
                y: 140.0,
                spans: vec![span("E", 30.0, 140.0)],
            },
        ];

        let config = TableDetectorConfig::default();
        let cols = detect_columns(&rows, &config);
        // x=10 and x=60 each appear in 2/3 rows >= 0.5 => kept.
        // x=30 appears in 1/3 rows < 0.5 => dropped.
        assert_eq!(cols.len(), 2);
    }

    // -- detect_tables (integration) ---------------------------------------

    #[test]
    fn detect_tables_valid_table() {
        let spans = vec![
            // Row 1
            span("Name", 10.0, 100.0),
            span("Age", 80.0, 100.0),
            span("City", 150.0, 100.0),
            // Row 2
            span("Alice", 10.0, 120.0),
            span("30", 80.0, 120.0),
            span("NYC", 150.0, 120.0),
            // Row 3
            span("Bob", 10.0, 140.0),
            span("25", 80.0, 140.0),
            span("LA", 150.0, 140.0),
        ];

        let config = TableDetectorConfig::default();
        let tables = detect_tables(&spans, &config);
        assert_eq!(tables.len(), 1);
        let table = &tables[0];
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.rows.len(), 3);
    }

    #[test]
    fn detect_tables_too_few_rows() {
        let spans = vec![span("A", 10.0, 100.0), span("B", 60.0, 100.0)];
        let config = TableDetectorConfig::default();
        let tables = detect_tables(&spans, &config);
        assert!(tables.is_empty());
    }

    #[test]
    fn detect_tables_too_few_columns() {
        // Three rows but only one column.
        let spans = vec![
            span("A", 10.0, 100.0),
            span("B", 10.0, 120.0),
            span("C", 10.0, 140.0),
        ];
        let config = TableDetectorConfig::default();
        let tables = detect_tables(&spans, &config);
        assert!(tables.is_empty());
    }

    #[test]
    fn detect_tables_empty_input() {
        let config = TableDetectorConfig::default();
        let tables = detect_tables(&[], &config);
        assert!(tables.is_empty());
    }

    // -- Default config values ---------------------------------------------

    #[test]
    fn default_config_values() {
        let cfg = TableDetectorConfig::default();
        assert_eq!(cfg.min_rows, 2);
        assert_eq!(cfg.min_columns, 2);
        assert_eq!(cfg.max_columns, 20);
        assert!((cfg.y_tolerance_factor - 0.3).abs() < f32::EPSILON);
        assert!((cfg.min_alignment_ratio - 0.5).abs() < f32::EPSILON);
        assert!((cfg.min_column_gap - 10.0).abs() < f32::EPSILON);
    }

    // -- classify_blocks ---------------------------------------------------

    #[test]
    fn classify_heading_block() {
        let block = text_block(
            vec![text_line(vec![span("Introduction", 10.0, 100.0)])],
            BlockType::Heading(1),
        );
        let result = classify_blocks(vec![(1, vec![block])]);
        assert_eq!(result.len(), 1);
        match &result[0] {
            ClassifiedBlock::Heading { level, title, page } => {
                assert_eq!(*level, 1);
                assert_eq!(title, "Introduction");
                assert_eq!(*page, 1);
            }
            other => panic!("expected Heading, got {:?}", other),
        }
    }

    #[test]
    fn classify_paragraph_block() {
        let block = text_block(
            vec![
                text_line(vec![span("Hello", 10.0, 100.0)]),
                text_line(vec![span("world", 10.0, 120.0)]),
            ],
            BlockType::Paragraph,
        );
        let result = classify_blocks(vec![(2, vec![block])]);
        assert_eq!(result.len(), 1);
        match &result[0] {
            ClassifiedBlock::Paragraph { text, page } => {
                assert_eq!(text, "Hello world");
                assert_eq!(*page, 2);
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn classify_list_item_as_paragraph() {
        let block = text_block(
            vec![text_line(vec![span("- item", 10.0, 100.0)])],
            BlockType::ListItem,
        );
        let result = classify_blocks(vec![(1, vec![block])]);
        assert_eq!(result.len(), 1);
        match &result[0] {
            ClassifiedBlock::Paragraph { text, page } => {
                assert_eq!(text, "- item");
                assert_eq!(*page, 1);
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn classify_table_block_with_valid_grid() {
        // Build a block whose spans form a clear 3-column, 3-row table.
        let lines = vec![
            text_line(vec![
                span("H1", 10.0, 100.0),
                span("H2", 80.0, 100.0),
                span("H3", 150.0, 100.0),
            ]),
            text_line(vec![
                span("A1", 10.0, 120.0),
                span("A2", 80.0, 120.0),
                span("A3", 150.0, 120.0),
            ]),
            text_line(vec![
                span("B1", 10.0, 140.0),
                span("B2", 80.0, 140.0),
                span("B3", 150.0, 140.0),
            ]),
        ];
        let block = text_block(lines, BlockType::Table);
        let result = classify_blocks(vec![(5, vec![block])]);
        assert_eq!(result.len(), 1);
        match &result[0] {
            ClassifiedBlock::Table {
                headers,
                rows,
                page,
            } => {
                assert_eq!(*page, 5);
                assert_eq!(headers.len(), 3);
                assert_eq!(headers[0], "H1");
                assert_eq!(headers[1], "H2");
                assert_eq!(headers[2], "H3");
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0], "A1");
                assert_eq!(rows[1][2], "B3");
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn classify_table_block_falls_back_to_paragraph() {
        // A Table block that does not have enough structure for detection.
        let block = text_block(
            vec![text_line(vec![span("just text", 10.0, 100.0)])],
            BlockType::Table,
        );
        let result = classify_blocks(vec![(1, vec![block])]);
        assert_eq!(result.len(), 1);
        match &result[0] {
            ClassifiedBlock::Paragraph { text, page } => {
                assert_eq!(text, "just text");
                assert_eq!(*page, 1);
            }
            other => panic!("expected Paragraph fallback, got {:?}", other),
        }
    }

    // -- table_to_grid helper ----------------------------------------------

    #[test]
    fn table_to_grid_produces_headers_and_rows() {
        let table = DetectedTable {
            top_y: 100.0,
            bottom_y: 140.0,
            left_x: 10.0,
            right_x: 200.0,
            columns: vec![10.0, 80.0],
            rows: vec![
                TableRowData {
                    y: 100.0,
                    spans: vec![span("Name", 10.0, 100.0), span("Age", 80.0, 100.0)],
                },
                TableRowData {
                    y: 120.0,
                    spans: vec![span("Alice", 10.0, 120.0), span("30", 80.0, 120.0)],
                },
            ],
        };

        let (headers, rows) = table_to_grid(&table);
        assert_eq!(headers, vec!["Name", "Age"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["Alice", "30"]);
    }

    // -- compute_y_tolerance -----------------------------------------------

    #[test]
    fn y_tolerance_uses_median_font_size() {
        let spans = vec![
            span_with_size("A", 0.0, 0.0, 8.0),
            span_with_size("B", 0.0, 0.0, 10.0),
            span_with_size("C", 0.0, 0.0, 12.0),
        ];
        let tol = compute_y_tolerance(&spans, 0.3);
        // median font size = 10.0, tolerance = 10.0 * 0.3 = 3.0
        assert!((tol - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn y_tolerance_minimum_is_one() {
        let spans = vec![span_with_size("A", 0.0, 0.0, 0.5)];
        let tol = compute_y_tolerance(&spans, 0.3);
        // 0.5 * 0.3 = 0.15 which is below 1.0 minimum.
        assert!((tol - 1.0).abs() < f32::EPSILON);
    }

    // -- assign_column helper ----------------------------------------------

    #[test]
    fn assign_column_nearest() {
        let cols = vec![10.0, 60.0, 110.0];
        assert_eq!(assign_column(12.0, &cols), 0);
        assert_eq!(assign_column(55.0, &cols), 1);
        assert_eq!(assign_column(100.0, &cols), 2);
    }
}
