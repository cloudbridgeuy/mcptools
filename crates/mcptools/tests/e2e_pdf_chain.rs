mod common;

const SECTION_A: &str = "s-1-0";
const SECTION_B: &str = "s-1-1";
const SECTION_B_MARKER: &str = "PDF_CHAIN_SECTION_B_MARKER";
const PDF_SCRIPT: &str = "const toc = await pdf_toc({ path: PDF_PATH });\nconst sectionB = await pdf_read({ path: PDF_PATH, sectionId: SECTION_B });\nconst imgs = await pdf_images({ path: PDF_PATH, sectionId: SECTION_A });\nconsole.log(toc.sections.length, sectionB.text.includes(SECTION_B_MARKER), imgs.images.length);";

#[tokio::test]
async fn execute_script_scopes_pdf_calls_by_section() {
    let dir = tempfile::tempdir().expect("temp dir");
    let fixture = dir.path().join("chain.pdf");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chain.pdf"),
        &fixture,
    )
    .expect("copy fixture into temp dir");
    let path = fixture.to_str().expect("utf-8 temp path").to_string();

    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");

    let code = format!(
        "const PDF_PATH = {path:?};\nconst SECTION_A = {SECTION_A:?};\nconst SECTION_B = {SECTION_B:?};\nconst SECTION_B_MARKER = {SECTION_B_MARKER:?};\n{PDF_SCRIPT}"
    );
    let executed = server
        .tools_call("execute", serde_json::json!({"code": code}))
        .await;
    assert_eq!(
        executed["structuredContent"]["logs"],
        serde_json::json!(["2 true 1"])
    );
    assert_eq!(
        executed["structuredContent"]["result"],
        serde_json::json!(null)
    );

    let section_b_images = server
        .tools_call(
            "pdf_images",
            serde_json::json!({"path": path.as_str(), "sectionId": SECTION_B}),
        )
        .await;
    assert_eq!(
        section_b_images["structuredContent"]["images"],
        serde_json::json!([])
    );

    let all_images = server
        .tools_call("pdf_images", serde_json::json!({"path": path.as_str()}))
        .await;
    assert_eq!(
        all_images["structuredContent"]["images"]
            .as_array()
            .expect("images array")
            .len(),
        1
    );
}
