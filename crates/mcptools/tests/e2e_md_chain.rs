mod common;

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHAIN_SCRIPT: &str = r#"
const toc = await md_toc({ url: PAGE_URL });
const beta = toc.entries.find((entry) => entry.text.includes("Beta"));
const section = await md_fetch({ url: PAGE_URL, offset: beta.char_offset, limit: beta.char_limit });
console.log(toc.title, toc.entries.length, section.content.includes("BETA MARKER"), section.content.includes("ALPHA MARKER"));
"#;

#[tokio::test]
async fn md_chain_fetches_the_section_marked_by_toc() {
    let stub = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/md_chain.html"), "text/html"),
        )
        .mount(&stub)
        .await;

    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");
    let page_url = serde_json::to_string(&stub.uri()).expect("serialize stub uri");
    let code = format!("const PAGE_URL = {page_url};\n{CHAIN_SCRIPT}");
    let executed = server
        .tools_call("execute", serde_json::json!({"code": code}))
        .await;
    assert_eq!(
        executed["structuredContent"]["error"],
        serde_json::json!(null),
        "execute result: {executed}"
    );
    assert_eq!(
        executed["structuredContent"]["logs"],
        serde_json::json!(["Chain Doc 2 true false"]),
        "execute result: {executed}"
    );
    assert_eq!(
        executed["structuredContent"]["result"],
        serde_json::json!(null)
    );
}
