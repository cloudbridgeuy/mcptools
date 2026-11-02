use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderArgs {
    pub markdown: String,
    pub path: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RenderOutput {
    pub path: String,
    pub url: String,
}

pub struct RenderAssets<'a> {
    pub css: &'a str,
    pub mermaid_js: &'a str,
}

fn slug(text: &str) -> String {
    let slug = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    if slug.is_empty() {
        "markdown".to_owned()
    } else {
        slug
    }
}

pub fn render_filename(title: &str, html: &str) -> String {
    let slug: String = slug(title).chars().take(48).collect();
    let hash = format!("{:x}", md5::compute(html));
    format!("{slug}-{}.html", &hash[..12])
}

fn safe_link(destination: &str) -> bool {
    let normalized: String = destination
        .chars()
        .filter(|c| !c.is_ascii_control() && !c.is_ascii_whitespace())
        .collect();
    match reqwest::Url::parse(&normalized) {
        Ok(url) => matches!(url.scheme(), "https" | "http" | "mailto"),
        Err(_) => !normalized.contains(':'),
    }
}

pub fn render_markdown_html(markdown: &str, title: &str, assets: RenderAssets<'_>) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let events: Vec<_> = Parser::new_ext(markdown, options).collect();
    let mut anchors = HashSet::new();
    let mut mermaid = false;
    let transformed = events
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, event)| match event {
            Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                let text: String = events[index + 1..]
                    .iter()
                    .take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_))))
                    .filter_map(|event| match event {
                        Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                        Event::SoftBreak | Event::HardBreak => Some(" "),
                        _ => None,
                    })
                    .collect();
                let base = slug(&text);
                let mut anchor = base.clone();
                let mut suffix = 2;
                while !anchors.insert(anchor.clone()) {
                    anchor = format!("{base}-{suffix}");
                    suffix += 1;
                }
                Event::Start(Tag::Heading {
                    level,
                    id: Some(anchor.into()),
                    classes,
                    attrs,
                })
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if language.split_whitespace().next() == Some("mermaid") =>
            {
                mermaid = true;
                Event::Html("<pre class=\"mermaid\">".into())
            }
            Event::End(TagEnd::CodeBlock) if mermaid => {
                mermaid = false;
                Event::Html("</pre>\n".into())
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => Event::Start(Tag::Link {
                link_type,
                dest_url: if safe_link(&dest_url) {
                    dest_url
                } else {
                    "".into()
                },
                title,
                id,
            }),
            event => event,
        });
    let mut body = String::new();
    html::push_html(&mut body, transformed);
    let title = html_escape::encode_text(title);
    let css = assets.css;
    let js = assets
        .mermaid_js
        .replace("</script", "<\\/script")
        .replace("</SCRIPT", "<\\/SCRIPT");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\">\n<title>{title}</title>\n<style>{css}</style>\n</head>\n<body>\n<main>\n{body}</main>\n<script>{js}</script>\n<script>mermaid.initialize({{startOnLoad: true, securityLevel: 'strict'}});</script>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_html_matches_golden() {
        let rendered = render_markdown_html(
            include_str!("../../tests/fixtures/render.md"),
            "Fixture <report>",
            RenderAssets {
                css: "test-css",
                mermaid_js: "test-js",
            },
        );
        assert_eq!(rendered, include_str!("../../tests/fixtures/render.html"));
    }

    fn render(markdown: &str) -> String {
        render_markdown_html(
            markdown,
            "Title",
            RenderAssets {
                css: "",
                mermaid_js: "",
            },
        )
    }

    #[test]
    fn headings_are_unique_and_extensions_render() {
        let output = render("# Same *heading*\n\n# Same heading\n\n# Same heading-2\n\n~~gone~~[^note]\n\n[^note]: Footnote text\n");
        assert!(output.contains("id=\"same-heading\""));
        assert!(output.contains("id=\"same-heading-2\""));
        assert!(output.contains("id=\"same-heading-2-2\""));
        assert!(output.contains("<del>gone</del>"));
        assert!(output.contains("class=\"footnote-definition\""));
        assert!(output.contains("Footnote text"));
    }

    #[test]
    fn untrusted_html_and_links_cannot_execute_script() {
        let input = "# <img src=x onerror=alert(1)>\n\nInline <script>alert(1)</script>.\n\n[x](javascript:alert%281%29)\n\n```mermaid\n</pre><script>alert(1)</script>\n```\n";
        let output = render(input);
        assert!(!output.contains("<img src=x"));
        assert!(!output.contains("<script>alert(1)"));
        assert!(output.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(output.contains("<a href=\"\">x</a>"));
        assert_eq!(output, render(input));
    }

    #[test]
    fn filenames_are_safe_bounded_and_content_addressed() {
        let name = render_filename("../../ Report", "one");
        assert!(name.starts_with("report-"));
        assert!(name.ends_with(".html"));
        assert_eq!(name, render_filename("../../ Report", "one"));
        assert_ne!(name, render_filename("../../ Report", "two"));
        assert!(render_filename(&"界".repeat(200), "one").len() < 255);
        assert!(render_filename("", "one").starts_with("markdown-"));
    }
}
