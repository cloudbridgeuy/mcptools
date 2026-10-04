use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn tools_list() -> serde_json::Value {
    let mut child = Command::new(binary())
        .args(["mcp", "stdio"])
        .env_remove("MCPTOOLS_DISCOVERY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
    });
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

fn tools(response: &serde_json::Value) -> Vec<serde_json::Value> {
    response
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .unwrap()
        .to_vec()
}

fn find<'a>(tools: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
        .unwrap_or_else(|| panic!("missing tool {name}"))
}

fn required(input_schema: &serde_json::Value) -> Vec<String> {
    let mut req: Vec<String> = input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    req.sort();
    req
}

fn assert_dual_envelope(structured: serde_json::Value) {
    let text = serde_json::to_string_pretty(&structured).unwrap();
    let result = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
    });
    let parsed: serde_json::Value = serde_json::from_str(
        result
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("text"))
            .and_then(|v| v.as_str())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(&parsed, result.get("structuredContent").unwrap());
}

fn sample(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contract_samples")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing recorded sample for {name}"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("bad JSON sample for {name}"))
}

#[test]
fn mcp_contract_rest_typed_outputs() {
    let response = tools_list();
    let all = tools(&response);

    let props = find(&all, "pdf_toc")
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("sections"));
    let props = find(&all, "ui_annotations_list")
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("annotations"));

    let recorded = sample("pdf_toc");
    let output: pdf::DocumentTree = serde_json::from_value(recorded).unwrap();
    assert_eq!(output.title, "Doc");
    assert_eq!(output.sections.len(), 1);
    assert_dual_envelope(serde_json::to_value(&output).unwrap());

    let recorded = sample("pdf_images");
    let output: pdf::PdfImagesOutput = serde_json::from_value(recorded.clone()).unwrap();
    assert_eq!(output.images.len(), 1);
    assert_eq!(output.images[0].section_title, "Intro");
    assert_eq!(&serde_json::to_value(&output).unwrap(), &recorded);
    assert_dual_envelope(recorded);

    let recorded = sample("images_generate");
    let files = recorded.get("files").and_then(|v| v.as_array()).unwrap();
    assert_eq!(files.len(), 1);
    for key in ["files", "model", "size", "quality", "output_format"] {
        assert!(
            recorded.get(key).is_some(),
            "missing {key} in images_generate"
        );
    }
    assert_dual_envelope(recorded);

    let recorded = sample("ui_annotations_list");
    let output: mcptools_core::annotations::ListAnnotationsResponse =
        serde_json::from_value(recorded).unwrap();
    assert_eq!(output.annotations.len(), 1);
    assert_eq!(output.annotations[0].id, "abc-123");
    assert_dual_envelope(serde_json::to_value(&output).unwrap());

    let recorded = sample("ui_annotations_resolve");
    assert_eq!(
        recorded.get("resolved").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_dual_envelope(recorded);

    let recorded = sample("ui_annotations_clear");
    assert!(recorded.get("cleared").and_then(|v| v.as_u64()).is_some());
    assert_dual_envelope(recorded);
}

fn recorded_hn_post() -> serde_json::Value {
    serde_json::json!({
        "id": 8863,
        "title": "Sample story",
        "url": "https://example.com",
        "author": "pg",
        "score": 100,
        "time": "2021-01-01 00:00:00 UTC",
        "text": "Story text",
        "total_comments": 1,
        "comments": [
            {
                "id": 9001,
                "author": "sama",
                "time": "2021-01-01 01:00:00 UTC",
                "text": "Nice post",
                "replies_count": 0,
            }
        ],
        "pagination": {
            "current_page": 1,
            "total_pages": 1,
            "total_comments": 1,
            "limit": 10,
            "next_page_command": serde_json::Value::Null,
            "prev_page_command": serde_json::Value::Null,
        },
    })
}

fn recorded_md_fetch() -> serde_json::Value {
    serde_json::json!({
        "url": "https://example.com",
        "title": "Example",
        "content": "# Hello",
        "html_length": 128,
        "fetch_time_ms": 12,
        "pagination": {
            "current_page": 1,
            "total_pages": 1,
            "total_characters": 7,
            "limit": 1000,
            "has_more": false,
        },
    })
}

fn recorded_pdf_section() -> serde_json::Value {
    serde_json::json!({
        "id": "s-1-0",
        "title": "Intro",
        "text": "Hello pdf",
        "images": [{"id": "im0", "format": "Png"}],
    })
}

fn recorded_images_usage() -> serde_json::Value {
    serde_json::json!({
        "input_tokens": 10,
        "output_tokens": 20,
        "total_tokens": 30,
        "input_tokens_details": {"image_tokens": 8, "text_tokens": 2},
    })
}

fn recorded_annotation() -> serde_json::Value {
    serde_json::json!({
        "id": "abc-123",
        "timestamp": "2026-03-03T17:40:15Z",
        "tag_name": "div",
        "text_content": "Hello",
        "element_path": "div.grid > div.entry",
        "comment": "Fix the font",
        "is_fixed": false,
        "component_name": "Entry",
        "bounding_box": {"top": 10.0, "left": 20.0, "width": 100.0, "height": 50.0},
        "computed_styles": {
            "color": "rgb(0, 0, 0)",
            "background_color": "rgb(255, 255, 255)",
            "font_size": "16px",
            "font_family": "Inter",
            "padding": "8px",
            "margin": "0px",
            "width": "200px",
            "height": "40px",
            "display": "flex",
            "position": "relative"
        },
    })
}

#[test]
fn mcp_contract_rest() {
    let response = tools_list();
    let all = tools(&response);
    assert_eq!(all.len(), 64, "unexpected tools/list length");
    for tool in &all {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let output = tool
            .get("outputSchema")
            .unwrap_or_else(|| panic!("missing outputSchema in {name}"));
        assert_eq!(
            output.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad outputSchema type in {name}"
        );
        let input = tool
            .get("inputSchema")
            .unwrap_or_else(|| panic!("missing inputSchema in {name}"));
        assert_eq!(
            input.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad inputSchema type in {name}"
        );
    }

    let rest = [
        "hn_read_item",
        "hn_list_items",
        "md_fetch",
        "md_toc",
        "ui_annotations_list",
        "ui_annotations_get",
        "ui_annotations_resolve",
        "ui_annotations_clear",
        "pdf_toc",
        "pdf_read",
        "pdf_peek",
        "pdf_images",
        "pdf_image",
        "pdf_info",
        "images_generate",
        "images_edit",
        "images_vary",
    ];
    for name in rest {
        assert!(
            find(&all, name).get("outputSchema").is_some(),
            "missing outputSchema in {name}"
        );
    }

    assert_eq!(
        required(find(&all, "hn_read_item").get("inputSchema").unwrap()),
        vec!["item".to_string()]
    );
    assert_eq!(
        required(find(&all, "images_generate").get("inputSchema").unwrap()),
        vec!["prompt".to_string()]
    );
    assert_eq!(
        required(find(&all, "pdf_image").get("inputSchema").unwrap()),
        vec!["path".to_string()]
    );
    assert_eq!(
        required(find(&all, "confluence_search").get("inputSchema").unwrap()),
        vec!["query".to_string()]
    );

    let props = find(&all, "pdf_images")
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("images"));
    let props = find(&all, "images_generate")
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("files"));
    let props = find(&all, "ui_annotations_resolve")
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("id"));

    let sample = recorded_hn_post();
    let output: mcptools_core::hn::PostOutput = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.id, 8863);
    assert_eq!(output.comments.len(), 1);
    assert_eq!(&serde_json::to_value(&output).unwrap(), &sample);
    assert_dual_envelope(sample);

    let sample = recorded_md_fetch();
    let output: mcptools_core::md::FetchOutput = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.url, "https://example.com");
    assert_eq!(&serde_json::to_value(&output).unwrap(), &sample);
    assert_dual_envelope(sample);

    let sample = recorded_pdf_section();
    let output: pdf::SectionContent = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.title, "Intro");
    assert_eq!(output.images.len(), 1);
    assert_eq!(&serde_json::to_value(&output).unwrap(), &sample);
    assert_dual_envelope(sample);

    let sample = recorded_images_usage();
    let output: mcptools_core::images::Usage = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.total_tokens, 30);
    assert_eq!(&serde_json::to_value(&output).unwrap(), &sample);
    assert_dual_envelope(sample);

    let sample = recorded_annotation();
    let output: mcptools_core::annotations::DevAnnotation = serde_json::from_value(sample).unwrap();
    assert_eq!(output.id, "abc-123");
    assert!(!output.resolved);
    let structured = serde_json::to_value(&output).unwrap();
    assert_dual_envelope(structured);
}
