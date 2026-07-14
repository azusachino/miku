//! Shared Markdown helpers used by both the HTTP renderer (`main.rs`) and the
//! background indexer (`indexer.rs`): frontmatter parsing, title extraction,
//! wikilink normalization, and HTML rendering with resolved wikilinks/embeds.
//!
//! Keeping parse and render on the *same* comrak options is load-bearing: the
//! indexer records which `[[links]]`/`![[embeds]]` exist, and the renderer must
//! tokenize them identically or the index and the page would disagree.

use comrak::nodes::{AstNode, NodeCode, NodeMath, NodeValue};
use comrak::Anchorizer;
use regex::Regex;
use std::path::Path;

lazy_static::lazy_static! {
    /// Matches `![[target]]` and `![[target|label]]` embeds. Comrak does *not*
    /// recognize the `![[...]]` form (it leaves the whole run as a text node),
    /// so embeds are detected by regex over text nodes in both the indexer and
    /// the renderer — never as `WikiLink` AST nodes.
    pub static ref EMBED_REGEX: Regex =
        Regex::new(r"!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap();

    /// Matches inline `#hashtags`. Kept identical to the indexer's tag regex so
    /// the links the renderer emits resolve to the same tags the indexer stored.
    /// Capture group 1 is the tag name (the leading delimiter is part of match 0
    /// but not the tag).
    pub static ref TAG_REGEX: Regex =
        Regex::new(r"(?:\s|^|['`\(])#([\p{L}\p{N}][\p{L}\p{N}_\-/]*)").unwrap();
}

/// Split YAML frontmatter from the Markdown body. Returns the parsed
/// frontmatter (as JSON) and the body slice after the closing `---`.
pub fn parse_frontmatter(content: &str) -> (Option<serde_json::Value>, &str) {
    if content.starts_with("---\n") || content.starts_with("---\r\n") {
        let header_len = if content.starts_with("---\n") { 4 } else { 5 };
        let rest = &content[header_len..];
        let mut search_idx = 0;
        while let Some(idx) = rest[search_idx..].find("\n---") {
            let actual_idx = search_idx + idx;
            let after = &rest[actual_idx + 4..];
            if after.is_empty() || after.starts_with('\n') || after.starts_with('\r') {
                let yaml_str = &rest[..actual_idx];
                let after_first_nl = after.find('\n').map_or(0, |n| n + 1);
                let body_str = &after[after_first_nl..];
                if let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(yaml_str) {
                    return (Some(yaml), body_str);
                }
                break;
            }
            search_idx = actual_idx + 4;
        }
    }
    (None, content)
}

/// Title resolution: frontmatter `title`, else the first `# H1`, else the
/// path basename.
pub fn extract_title(path: &str, frontmatter: Option<&serde_json::Value>, body: &str) -> String {
    if let Some(fm) = frontmatter {
        if let Some(title) = fm.get("title").and_then(|t| t.as_str()) {
            return title.to_string();
        }
    }

    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("# ") {
            return stripped.trim().to_string();
        }
    }

    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// True when a link target names a binary asset (embedded with `![[file.png]]`)
/// rather than another page.
pub fn is_asset_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".svg")
        || lower.ends_with(".pdf")
        || lower.ends_with(".webp")
}

/// Normalize a wikilink target into the resolver key stored in `tb_pages.slug`
/// / `tb_links.target_norm`: lowercased, with a trailing `.md` stripped for
/// pages (assets keep their extension).
pub fn normalize_target(name: &str, is_asset: bool) -> String {
    let trimmed = name.trim();
    if is_asset {
        trimmed.to_lowercase()
    } else {
        let stripped = if trimmed.to_lowercase().ends_with(".md") {
            &trimmed[..trimmed.len() - 3]
        } else {
            trimmed
        };
        stripped.to_lowercase()
    }
}

/// Comrak options shared by parse (indexer) and render (HTTP). Render-only
/// flags (`render.r#unsafe`) are layered on top by [`render_html`].
pub fn comrak_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.alerts = true;
    options.extension.wikilinks_title_after_pipe = true;
    options.extension.header_id_prefix = Some(String::new());
    options
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub id: String,
}

/// Concatenate the visible text of a wikilink node's children (its display
/// label / alias).
fn node_label<'a>(node: &'a AstNode<'a>) -> String {
    let mut label = String::new();
    for child in node.children() {
        for desc in child.descendants() {
            if let NodeValue::Text(t) = &desc.data.borrow().value {
                label.push_str(t);
            }
        }
    }
    label
}

fn heading_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    heading_text_append(node, &mut text);
    text
}

fn heading_text_append<'a>(node: &'a AstNode<'a>, output: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(literal) => output.push_str(literal),
        NodeValue::Code(NodeCode { literal, .. }) | NodeValue::Math(NodeMath { literal, .. }) => {
            output.push_str(literal);
        }
        NodeValue::LineBreak | NodeValue::SoftBreak => output.push(' '),
        _ => {
            for child in node.children() {
                heading_text_append(child, output);
            }
        }
    }
}

fn collect_toc<'a>(root: &'a AstNode<'a>) -> Vec<Heading> {
    let mut anchorizer = Anchorizer::new();
    let mut headings = Vec::new();

    for node in root.descendants() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value {
            let text = heading_text(node);
            let id = anchorizer.anchorize(&text);
            headings.push(Heading {
                level: heading.level,
                text,
                id,
            });
        }
    }

    headings
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Replace a node with a single raw-HTML inline, dropping its children so only
/// the injected markup renders.
fn set_html_inline<'a>(node: &'a AstNode<'a>, html: String) {
    let children: Vec<_> = node.children().collect();
    for c in children {
        c.detach();
    }
    node.data.borrow_mut().value = NodeValue::HtmlInline(html);
}

/// Replace a block-level node with raw-HTML block markup, dropping its children.
fn set_html_block<'a>(node: &'a AstNode<'a>, html: String) {
    let children: Vec<_> = node.children().collect();
    for c in children {
        c.detach();
    }
    node.data.borrow_mut().value = NodeValue::HtmlBlock(comrak::nodes::NodeHtmlBlock {
        block_type: 0,
        literal: html,
    });
}

/// Render `[[Target]]` / `[[Target|label]]` as an anchor to the `/p/` page
/// route, tagged `wikilink-missing` when `resolved` reports the slug is absent.
fn anchor_html(target: &str, label: &str, resolved: &dyn Fn(&str) -> Option<String>) -> String {
    let label = if label.is_empty() { target } else { label };
    let norm = normalize_target(target, false);
    match resolved(&norm) {
        Some(actual_path) => {
            format!(
                "<a href=\"{}\" class=\"wikilink\">{}</a>",
                escape_attr(&format!("/p/{actual_path}")),
                escape_text(label)
            )
        }
        None => {
            format!(
                "<a href=\"{}\" class=\"wikilink wikilink-missing\">{}</a>",
                escape_attr(&format!("/p/{}", target.trim())),
                escape_text(label)
            )
        }
    }
}

/// Render an inline `#tag` as a link to its `/tags/{tag}` filter page.
fn tag_html(tag: &str) -> String {
    format!(
        "<a href=\"/tags/{}\" class=\"tag-inline\">#{}</a>",
        escape_attr(tag),
        escape_text(tag)
    )
}

fn render_admonition(
    info: &str,
    literal: &str,
    resolved: &dyn Fn(&str) -> Option<String>,
) -> String {
    let ad_type = info.strip_prefix("ad-").unwrap_or(info);
    let mut title = format!("{}{}", &ad_type[..1].to_uppercase(), &ad_type[1..]);
    let mut actual_content = String::new();
    let mut lines = literal.lines();

    if let Some(first_line) = lines.next() {
        let trimmed = first_line.trim_start();
        if let Some(stripped) = trimmed.strip_prefix("title:") {
            let t = stripped.trim();
            let t_unquoted = if (t.starts_with('"') && t.ends_with('"'))
                || (t.starts_with('\'') && t.ends_with('\''))
            {
                &t[1..t.len() - 1]
            } else {
                t
            };
            if !t_unquoted.is_empty() {
                title = t_unquoted.to_string();
            }
            for line in lines {
                actual_content.push_str(line);
                actual_content.push('\n');
            }
        } else {
            actual_content.push_str(first_line);
            actual_content.push('\n');
            for line in lines {
                actual_content.push_str(line);
                actual_content.push('\n');
            }
        }
    }

    let inner_html = render_html(&actual_content, resolved);

    let (icon, border_class, title_class) = match ad_type.to_lowercase().as_str() {
        "note" | "info" | "todo" => ("💡", "border-teal bg-teal/5", "text-teal"),
        "warning" | "warn" | "caution" => {
            ("⚠️", "border-amber-500 bg-amber-500/5", "text-amber-500")
        }
        "danger" | "error" | "fail" | "failure" => {
            ("🛑", "border-rose-500 bg-rose-500/5", "text-rose-500")
        }
        _ => ("📝", "border-muted bg-muted/5", "text-muted"),
    };

    format!(
        "<div class=\"admonition ad-{} border-l-4 p-3.5 my-4 {} rounded-r\">\n  <div class=\"admonition-title font-bold text-sm mb-1.5 {} flex items-center gap-1.5 select-none\">\n    <span>{}</span> {}</div>\n  <div class=\"admonition-content text-[13px] leading-relaxed\">\n    {}\n  </div>\n</div>",
        ad_type,
        border_class,
        title_class,
        icon,
        escape_text(&title),
        inner_html
    )
}

/// True when a text node sits inside a link or code context, where inline
/// `#tags` must NOT be linkified (a `#frag` in a URL is not a tag; `#tag` in
/// code is literal). Headings DO allow tags (Obsidian-style) — kept in sync
/// with the indexer's skip rule so rendered tag links match the indexed set.
fn tag_skipped<'a>(node: &'a AstNode<'a>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match &parent.data.borrow().value {
            NodeValue::CodeBlock(_) | NodeValue::Link(_) => return true,
            _ => {}
        }
        current = parent.parent();
    }
    false
}

/// Rewrite a text node's `![[...]]` embeds and (when `allow_tags`) inline
/// `#hashtags` into HTML in a single left-to-right pass: asset embeds become
/// `<img>` under `/assets/`; page embeds fall back to a page link; tags become
/// links to `/tags/{tag}`. Surrounding text is HTML-escaped, since the result is
/// injected verbatim as raw HTML.
fn render_text_inlines(
    text: &str,
    resolved: &dyn Fn(&str) -> Option<String>,
    allow_tags: bool,
) -> String {
    struct Hit {
        start: usize,
        end: usize,
        html: String,
    }
    let mut hits: Vec<Hit> = Vec::new();

    for cap in EMBED_REGEX.captures_iter(text) {
        let m = cap.get(0).unwrap();
        let target = cap.get(1).map_or("", |x| x.as_str()).trim();
        let label = cap
            .get(2)
            .map(|x| x.as_str().trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(target);

        let html = if is_asset_path(target) {
            let src = format!("/assets/{}", normalize_target(target, true));
            format!(
                "<img src=\"{}\" alt=\"{}\" class=\"wiki-embed\">",
                escape_attr(&src),
                escape_attr(label)
            )
        } else {
            anchor_html(target, label, resolved)
        };
        hits.push(Hit {
            start: m.start(),
            end: m.end(),
            html,
        });
    }

    if allow_tags {
        for cap in TAG_REGEX.captures_iter(text) {
            let whole = cap.get(0).unwrap();
            let tag = cap.get(1).unwrap();
            // Skip a tag that overlaps an embed (e.g. inside its label).
            if hits
                .iter()
                .any(|h| tag.start() < h.end && h.start < tag.end())
            {
                continue;
            }
            // Match 0 is `<delim>#<tag>`; keep the delimiter but drop the `#`
            // (tag_html re-adds it) so a single `#docs` doesn't render as `##docs`.
            // The `#` is the single ASCII byte immediately before the capture.
            let delim = &text[whole.start()..tag.start() - 1];
            hits.push(Hit {
                start: whole.start(),
                end: whole.end(),
                html: format!("{}{}", escape_text(delim), tag_html(tag.as_str())),
            });
        }
    }

    hits.sort_by_key(|h| h.start);

    let mut out = String::new();
    let mut last = 0;
    for h in hits {
        if h.start < last {
            continue; // defensive: drop any overlap
        }
        out.push_str(&escape_text(&text[last..h.start]));
        out.push_str(&h.html);
        last = h.end;
    }
    out.push_str(&escape_text(&text[last..]));
    out
}

/// Render a Markdown body to HTML, rewriting `[[wikilinks]]` to `/p/` page
/// routes (with a `wikilink-missing` class when `resolved` reports the target
/// slug is absent) and `![[file.png]]` asset embeds to `<img>` under
/// `/assets/`.
///
/// `resolved` is called with a normalized page slug (see [`normalize_target`]).
/// Raw HTML passthrough is enabled — this is a single-user, local wiki over
/// trusted files, so users may hand-write HTML in their notes.
pub fn render_html_with_toc(
    body: &str,
    resolved: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<Heading>) {
    let arena = comrak::Arena::new();
    let mut options = comrak_options();
    options.render.r#unsafe = true;
    let root = comrak::parse_document(&arena, body, &options);
    let toc = collect_toc(root);

    // Collect first, then mutate: detaching children mid-traversal would
    // corrupt the descendants() iterator. `[[ ]]` links are WikiLink nodes;
    // `![[ ]]` embeds are never tokenized by comrak and surface as text.
    let nodes: Vec<&AstNode> = root.descendants().collect();
    for node in nodes {
        enum Rewrite {
            Link(String),
            // Inline text carrying embeds and/or tags; bool = linkify tags here.
            InlineText(String, bool),
            Mermaid(String),
            Admonition(String, String),
        }
        let allow_tags = !tag_skipped(node);
        let rewrite = match &node.data.borrow().value {
            NodeValue::WikiLink(w) => Some(Rewrite::Link(w.url.clone())),
            NodeValue::Text(t)
                if EMBED_REGEX.is_match(t) || (allow_tags && TAG_REGEX.is_match(t)) =>
            {
                Some(Rewrite::InlineText(t.to_string(), allow_tags))
            }
            NodeValue::CodeBlock(cb) => {
                let info = cb.info.split_whitespace().next().unwrap_or("");
                if info == "mermaid" {
                    Some(Rewrite::Mermaid(cb.literal.clone()))
                } else if info.starts_with("ad-") {
                    Some(Rewrite::Admonition(info.to_string(), cb.literal.clone()))
                } else {
                    None
                }
            }
            _ => None,
        };

        match rewrite {
            Some(Rewrite::Link(url)) => {
                let html = anchor_html(&url, &node_label(node), resolved);
                set_html_inline(node, html);
            }
            Some(Rewrite::InlineText(text, tags)) => {
                set_html_inline(node, render_text_inlines(&text, resolved, tags));
            }
            Some(Rewrite::Mermaid(src)) => {
                set_html_block(
                    node,
                    format!("<pre class=\"mermaid\">{}</pre>", escape_text(&src)),
                );
            }
            Some(Rewrite::Admonition(info, literal)) => {
                let html = render_admonition(&info, &literal, resolved);
                set_html_block(node, html);
            }
            None => {}
        }
    }

    // Writing to a String is infallible, so the formatter result is ignored.
    let mut out = String::new();
    let _ = comrak::format_html(root, &options, &mut out);
    (out, toc)
}

pub fn render_html(body: &str, resolved: &dyn Fn(&str) -> Option<String>) -> String {
    render_html_with_toc(body, resolved).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\ntitle: Hello World\n---\n# Header\nBody content";
        let (yaml, body) = parse_frontmatter(content);
        assert!(yaml.is_some());
        assert_eq!(
            yaml.unwrap().get("title").and_then(|t| t.as_str()),
            Some("Hello World")
        );
        assert_eq!(body, "# Header\nBody content");
    }

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "# Header\nBody content";
        let (yaml, body) = parse_frontmatter(content);
        assert!(yaml.is_none());
        assert_eq!(body, "# Header\nBody content");
    }

    #[test]
    fn test_render_page_link_resolved_vs_missing() {
        let html = render_html("See [[Foo]] and [[Bar]].", &|norm| {
            if norm == "foo" {
                Some("Foo".to_string())
            } else {
                None
            }
        });
        assert!(html.contains(r#"<a href="/p/Foo" class="wikilink">Foo</a>"#));
        assert!(html.contains(r#"<a href="/p/Bar" class="wikilink wikilink-missing">Bar</a>"#));
    }

    #[test]
    fn test_render_aliased_link() {
        let html = render_html("[[Target|Display]]", &|_| Some("Target".to_string()));
        assert!(html.contains(r#"<a href="/p/Target" class="wikilink">Display</a>"#));
    }

    #[test]
    fn test_render_asset_embed() {
        let html = render_html("![[diagram.png]]", &|_| None);
        assert!(html.contains(r#"<img src="/assets/diagram.png""#));
        // The leading `!` must not leak into the output.
        assert!(!html.contains(">!<"));
        assert!(!html.contains("!<img"));
    }

    #[test]
    fn test_render_inline_tag_linkified() {
        let html = render_html("Tagged with #feature and #guide here.", &|norm| {
            Some(norm.to_string())
        });
        assert!(html.contains(r#"<a href="/tags/feature" class="tag-inline">#feature</a>"#));
        assert!(html.contains(r#"<a href="/tags/guide" class="tag-inline">#guide</a>"#));
    }

    #[test]
    fn test_render_single_hash_no_double() {
        // A single `#docs` must not render the delimiter `#` plus the pill's `#`
        // (which read as `##docs`). Assert the exact surrounding text.
        let html = render_html("end #docs", &|norm| Some(norm.to_string()));
        assert!(html.contains(r#"end <a href="/tags/docs" class="tag-inline">#docs</a>"#));
        assert!(!html.contains("##docs"));
        assert!(!html.contains(r#"#<a href="/tags/docs""#));
    }

    #[test]
    fn test_render_double_hash_not_linkified() {
        // `##docs` is not a valid tag and must stay literal.
        let html = render_html("see ##docs here", &|norm| Some(norm.to_string()));
        assert!(!html.contains("tag-inline"));
        assert!(html.contains("##docs"));
    }

    #[test]
    fn test_render_tag_keeps_open_paren_delim() {
        let html = render_html("(#tag)", &|norm| Some(norm.to_string()));
        assert!(html.contains(r#"(<a href="/tags/tag" class="tag-inline">#tag</a>)"#));
    }

    #[test]
    fn test_render_tag_in_heading_linkified() {
        // Obsidian-style: #tags inside headings ARE linkified.
        let html = render_html("# Heading #docs\n", &|norm| Some(norm.to_string()));
        assert!(html.contains(r#"<a href="/tags/docs" class="tag-inline">#docs</a>"#));
    }

    #[test]
    fn test_render_tag_still_skipped_in_link() {
        // A `#frag` inside a link label is not a tag.
        let html = render_html("[#notag](https://example.com)", &|norm| {
            Some(norm.to_string())
        });
        assert!(!html.contains("tag-inline"));
    }

    #[test]
    fn test_render_tag_and_wikilink_together() {
        let html = render_html("See [[Index]] about #docs", &|norm| {
            if norm == "index" {
                Some("Index".to_string())
            } else {
                Some(norm.to_string())
            }
        });
        assert!(html.contains(r#"<a href="/p/Index" class="wikilink">Index</a>"#));
        assert!(html.contains(r#"<a href="/tags/docs" class="tag-inline">#docs</a>"#));
    }

    #[test]
    fn test_render_mermaid_block() {
        let html = render_html("```mermaid\ngraph TD; A-->B;\n```", &|norm| {
            Some(norm.to_string())
        });
        assert!(html.contains(r#"<pre class="mermaid">"#));
        assert!(html.contains("graph TD; A--&gt;B;"));
        // Must not be emitted as a Prism-highlightable code block.
        assert!(!html.contains("language-mermaid"));
    }

    #[test]
    fn test_render_non_mermaid_code_block_untouched() {
        let html = render_html("```rust\nfn main() {}\n```", &|norm| Some(norm.to_string()));
        assert!(html.contains("language-rust"));
        assert!(!html.contains(r#"class="mermaid""#));
    }

    #[test]
    fn test_render_page_embed_falls_back_to_link() {
        let html = render_html("![[Page]]", &|norm| {
            if norm == "page" {
                Some("Page".to_string())
            } else {
                Some(norm.to_string())
            }
        });
        assert!(html.contains(r#"<a href="/p/Page" class="wikilink">Page</a>"#));
        assert!(!html.contains("!<a"));
    }

    #[test]
    fn test_render_table() {
        let markdown = "| Col 1 | Col 2 |\n|---|---|\n| Val 1 | Val 2 |";
        let html = render_html(markdown, &|norm| Some(norm.to_string()));
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>Col 1</th>"));
        assert!(html.contains("<td>Val 1</td>"));
    }

    #[test]
    fn test_render_html_with_toc_matches_heading_ids() {
        let (html, toc) = render_html_with_toc(
            "# Intro\n\n## Hello **World**\n\n### Hello `World`\n\n## Hello World\n",
            &|norm| Some(norm.to_string()),
        );

        assert!(html.contains(
            r##"<a href="#intro" aria-hidden="true" class="anchor" id="intro"></a>Intro"##
        ));
        assert!(html.contains(r##"<a href="#hello-world" aria-hidden="true" class="anchor" id="hello-world"></a>Hello <strong>World</strong>"##));
        assert!(html.contains(r##"<a href="#hello-world-1" aria-hidden="true" class="anchor" id="hello-world-1"></a>Hello <code>World</code>"##));
        assert!(html.contains(r##"<a href="#hello-world-2" aria-hidden="true" class="anchor" id="hello-world-2"></a>Hello World"##));
        assert_eq!(
            toc,
            vec![
                Heading {
                    level: 1,
                    text: "Intro".to_string(),
                    id: "intro".to_string(),
                },
                Heading {
                    level: 2,
                    text: "Hello World".to_string(),
                    id: "hello-world".to_string(),
                },
                Heading {
                    level: 3,
                    text: "Hello World".to_string(),
                    id: "hello-world-1".to_string(),
                },
                Heading {
                    level: 2,
                    text: "Hello World".to_string(),
                    id: "hello-world-2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_render_admonition_block() {
        let md = "```ad-note\ntitle: Custom Note Title\nThis is **bold** text inside.\n```";
        let html = render_html(md, &|norm| Some(norm.to_string()));
        assert!(html.contains("admonition ad-note"));
        assert!(html.contains("Custom Note Title"));
        assert!(html.contains("<strong>bold</strong>"));
    }
}
