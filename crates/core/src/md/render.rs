use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::LazyLock;
use syntect::highlighting::ThemeSet;
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "syntax-" };
static SYNTAX_CSS: LazyLock<String> = LazyLock::new(|| {
    let themes = ThemeSet::load_defaults();
    let light = css_for_theme_with_class_style(&themes.themes["InspiredGitHub"], CLASS_STYLE)
        .expect("embedded light theme has valid scopes");
    let dark = css_for_theme_with_class_style(&themes.themes["base16-ocean.dark"], CLASS_STYLE)
        .expect("embedded dark theme has valid scopes");
    format!("{light}@media (prefers-color-scheme: dark) {{\n{dark}}}\n")
});

fn highlight_code(language: &str, code: &str) -> Option<String> {
    let syntax = SYNTAXES.find_syntax_by_token(language)?;
    let mut generator = ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, CLASS_STYLE);
    for line in LinesWithEndings::from(code) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .ok()?;
    }
    let language = html_escape::encode_double_quoted_attribute(language);
    Some(format!(
        "<pre class=\"syntax-code\"><code class=\"language-{language}\">{}</code></pre>\n",
        generator.finalize()
    ))
}

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
    let mut highlighted = false;
    let transformed = events
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(index, event)| {
            if highlighted {
                if matches!(event, Event::End(TagEnd::CodeBlock)) {
                    highlighted = false;
                }
                return None;
            }
            Some(match event {
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
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info))) => {
                    let code: String = events[index + 1..]
                        .iter()
                        .take_while(|event| !matches!(event, Event::End(TagEnd::CodeBlock)))
                        .filter_map(|event| match event {
                            Event::Text(text) => Some(text.as_ref()),
                            _ => None,
                        })
                        .collect();
                    match info
                        .split_whitespace()
                        .next()
                        .and_then(|language| highlight_code(language, &code))
                    {
                        Some(html) => {
                            highlighted = true;
                            Event::Html(html.into())
                        }
                        None => event,
                    }
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
            })
        });
    let mut body = String::new();
    html::push_html(&mut body, transformed);
    let title = html_escape::encode_text(title);
    let css = format!("{}{}", assets.css, *SYNTAX_CSS);
    let js = assets
        .mermaid_js
        .replace("</script", "<\\/script")
        .replace("</SCRIPT", "<\\/SCRIPT");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\">\n<title>{title}</title>\n<style>{css}</style>\n</head>\n<body>\n<main>\n{body}</main>\n<script>{js}</script>\n<script>mermaid.initialize({{startOnLoad: true, securityLevel: 'strict', theme: window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'default'}});</script>\n</body>\n</html>\n"
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
    fn code_is_highlighted_escaped_and_deterministic() {
        let input = "```rust extra-info\nlet value = \"</code><script>&\";\n```\n\n```js\nconst value = '<safe>';\n```\n\n```sh\necho '<safe>'\n```\n";
        let output = render(input);
        assert_eq!(output, render(input));
        assert_eq!(output.matches("<pre class=\"syntax-code\">").count(), 3);
        assert!(output.contains("syntax-keyword"));
        assert!(output.contains("&lt;/code&gt;&lt;script&gt;&amp;"));
        assert!(!output.contains("</code><script>"));
        assert!(!output.contains("style=\""));
        assert!(output.contains("@media (prefers-color-scheme: dark)"));
    }

    #[test]
    fn unknown_missing_and_indented_code_remain_plain() {
        let code = "<script>&</script>\n";
        let escaped = "&lt;script&gt;&amp;&lt;/script&gt;\n";
        for (input, opening) in [
            (
                format!("```not-a-language\n{code}```"),
                "<pre><code class=\"language-not-a-language\">",
            ),
            (format!("```\n{code}```"), "<pre><code>"),
            (format!("    {code}"), "<pre><code>"),
        ] {
            let output = render(&input);
            assert!(output.contains(&format!("{opening}{escaped}</code></pre>")));
            assert!(!output.contains("<span"));
        }
    }

    #[test]
    fn highlighted_blocks_preserve_multiline_and_unterminated_content() {
        let output = render("```rust\nlet text = \"first\n<second>&\";\n\n```\n\n```rust\n");
        assert_eq!(output.matches("<pre class=\"syntax-code\">").count(), 2);
        assert!(output.contains("first\n&lt;second&gt;&amp;"));
        assert!(output.contains("<code class=\"language-rust\"></code>"));
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
