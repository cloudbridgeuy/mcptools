use std::fmt::Write;

use serde::{Deserialize, Serialize};

/// A UI annotation from the calendsync dev overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevAnnotation {
    pub id: String,
    pub timestamp: String,
    pub selector: String,
    #[serde(default)]
    pub component_name: Option<String>,
    pub tag_name: String,
    pub text_content: String,
    pub note: String,
    pub bounding_box: BoundingBox,
    pub computed_styles: ComputedStyles,
    #[serde(default)]
    pub screenshot: Option<String>,
    pub resolved: bool,
    #[serde(default)]
    pub resolution_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedStyles {
    pub color: String,
    pub background_color: String,
    pub font_size: String,
    pub font_family: String,
    pub padding: String,
    pub margin: String,
    pub width: String,
    pub height: String,
    pub display: String,
    pub position: String,
}

/// API response for the list endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ListAnnotationsResponse {
    pub annotations: Vec<DevAnnotation>,
    pub summary: AnnotationSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationSummary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

/// Formats a single annotation as a concise markdown line for list output.
pub fn format_annotation(annotation: &DevAnnotation, index: usize) -> String {
    let component = annotation
        .component_name
        .as_deref()
        .map(|c| format!(" ({c})"))
        .unwrap_or_default();

    let status = if annotation.resolved {
        " [RESOLVED]"
    } else {
        ""
    };

    format!(
        "**#{}** [{}] `{}`{}{} — {}",
        index + 1,
        annotation.id,
        annotation.selector,
        component,
        status,
        annotation.note,
    )
}

/// Formats all annotations as a markdown summary for the `ui_annotations_list` tool.
pub fn format_annotations_list(
    annotations: &[DevAnnotation],
    summary: &AnnotationSummary,
) -> String {
    if annotations.is_empty() {
        return "No annotations found. The developer hasn't annotated any UI elements yet."
            .to_string();
    }

    let mut output = format!(
        "## UI Annotations ({} total, {} unresolved)\n\n",
        summary.total, summary.unresolved,
    );

    for (i, annotation) in annotations.iter().enumerate() {
        let _ = writeln!(output, "{}", format_annotation(annotation, i));
    }

    output.push_str(
        "\n---\nUse `ui_annotations_get` with an annotation ID for full details including styles and screenshot.",
    );

    output
}

/// Formats a single annotation with full detail for the `ui_annotations_get` tool.
pub fn format_annotation_detail(annotation: &DevAnnotation) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "## Annotation: {}\n", annotation.id);
    let _ = writeln!(output, "**Selector:** `{}`", annotation.selector);

    if let Some(ref component) = annotation.component_name {
        let _ = writeln!(output, "**Component:** {component}");
    }

    let _ = writeln!(output, "**Element:** `<{}>`", annotation.tag_name);

    if !annotation.text_content.is_empty() {
        let _ = writeln!(output, "**Text:** \"{}\"", annotation.text_content);
    }

    let _ = writeln!(output, "**Note:** {}", annotation.note);
    let _ = writeln!(output, "**Created:** {}", annotation.timestamp);

    if annotation.resolved {
        let _ = writeln!(output, "**Status:** Resolved");
        if let Some(ref summary) = annotation.resolution_summary {
            let _ = writeln!(output, "**Resolution:** {summary}");
        }
    } else {
        let _ = writeln!(output, "**Status:** Open");
    }

    let bb = &annotation.bounding_box;
    let _ = writeln!(
        output,
        "\n### Position\n- Top: {:.0}px, Left: {:.0}px\n- Size: {:.0}px x {:.0}px",
        bb.top, bb.left, bb.width, bb.height,
    );

    let cs = &annotation.computed_styles;
    let _ = writeln!(output, "\n### Computed Styles");
    let _ = writeln!(output, "- Color: `{}`", cs.color);
    let _ = writeln!(output, "- Background: `{}`", cs.background_color);
    let _ = writeln!(output, "- Font: `{}` at `{}`", cs.font_family, cs.font_size);
    let _ = writeln!(output, "- Padding: `{}`", cs.padding);
    let _ = writeln!(output, "- Margin: `{}`", cs.margin);
    let _ = writeln!(output, "- Display: `{}`", cs.display);
    let _ = writeln!(output, "- Position: `{}`", cs.position);

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bounding_box() -> BoundingBox {
        BoundingBox {
            top: 100.0,
            left: 200.0,
            width: 300.0,
            height: 50.0,
        }
    }

    fn sample_computed_styles() -> ComputedStyles {
        ComputedStyles {
            color: "rgb(0, 0, 0)".to_string(),
            background_color: "rgb(255, 255, 255)".to_string(),
            font_size: "16px".to_string(),
            font_family: "Inter, sans-serif".to_string(),
            padding: "8px".to_string(),
            margin: "0px".to_string(),
            width: "300px".to_string(),
            height: "50px".to_string(),
            display: "block".to_string(),
            position: "relative".to_string(),
        }
    }

    fn sample_annotation(id: &str, resolved: bool) -> DevAnnotation {
        DevAnnotation {
            id: id.to_string(),
            timestamp: "2024-01-15T10:00:00Z".to_string(),
            selector: "div.calendar > h1".to_string(),
            component_name: Some("CalendarHeader".to_string()),
            tag_name: "h1".to_string(),
            text_content: "January 2024".to_string(),
            note: "Font size too small on mobile".to_string(),
            bounding_box: sample_bounding_box(),
            computed_styles: sample_computed_styles(),
            screenshot: None,
            resolved,
            resolution_summary: if resolved {
                Some("Increased font size to 24px".to_string())
            } else {
                None
            },
        }
    }

    fn sample_summary(total: usize, resolved: usize) -> AnnotationSummary {
        AnnotationSummary {
            total,
            resolved,
            unresolved: total - resolved,
        }
    }

    #[test]
    fn test_format_annotation_unresolved() {
        let a = sample_annotation("abc", false);
        let result = format_annotation(&a, 0);
        assert_eq!(
            result,
            "**#1** [abc] `div.calendar > h1` (CalendarHeader) — Font size too small on mobile"
        );
    }

    #[test]
    fn test_format_annotation_resolved() {
        let a = sample_annotation("abc", true);
        let result = format_annotation(&a, 2);
        assert!(result.contains("[RESOLVED]"));
        assert!(result.starts_with("**#3**"));
    }

    #[test]
    fn test_format_annotation_no_component() {
        let mut a = sample_annotation("abc", false);
        a.component_name = None;
        let result = format_annotation(&a, 0);
        assert_eq!(
            result,
            "**#1** [abc] `div.calendar > h1` — Font size too small on mobile"
        );
    }

    #[test]
    fn test_format_annotations_list_empty() {
        let result = format_annotations_list(&[], &sample_summary(0, 0));
        assert!(result.contains("No annotations found"));
    }

    #[test]
    fn test_format_annotations_list_multiple() {
        let annotations = vec![sample_annotation("a", false), sample_annotation("b", true)];
        let result = format_annotations_list(&annotations, &sample_summary(2, 1));
        assert!(result.contains("2 total, 1 unresolved"));
        assert!(result.contains("**#1**"));
        assert!(result.contains("**#2**"));
        assert!(result.contains("ui_annotations_get"));
    }

    #[test]
    fn test_format_annotation_detail_unresolved() {
        let a = sample_annotation("abc-123", false);
        let result = format_annotation_detail(&a);
        assert!(result.contains("## Annotation: abc-123"));
        assert!(result.contains("**Selector:** `div.calendar > h1`"));
        assert!(result.contains("**Component:** CalendarHeader"));
        assert!(result.contains("**Status:** Open"));
        assert!(result.contains("Font: `Inter, sans-serif` at `16px`"));
        assert!(result.contains("Top: 100px, Left: 200px"));
    }

    #[test]
    fn test_format_annotation_detail_resolved() {
        let a = sample_annotation("abc-123", true);
        let result = format_annotation_detail(&a);
        assert!(result.contains("**Status:** Resolved"));
        assert!(result.contains("**Resolution:** Increased font size to 24px"));
    }

    #[test]
    fn test_format_annotation_detail_no_component() {
        let mut a = sample_annotation("abc-123", false);
        a.component_name = None;
        let result = format_annotation_detail(&a);
        assert!(!result.contains("**Component:**"));
    }

    #[test]
    fn test_format_annotation_detail_empty_text() {
        let mut a = sample_annotation("abc-123", false);
        a.text_content = String::new();
        let result = format_annotation_detail(&a);
        assert!(!result.contains("**Text:**"));
    }
}
