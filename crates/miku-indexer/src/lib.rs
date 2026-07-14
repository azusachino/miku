//! Filesystem-facing index projection helpers.
//!
//! This crate deliberately does not own a database connection. It turns one
//! Markdown document into a domain [`miku_domain::PageIndex`]; a concrete
//! [`miku_domain::IndexStore`] owns persistence and transaction semantics.

use miku_domain::{
    DocumentSignals, HeadingSummary, LinkKind, LinkRecord, PageIndex, PageSummary,
};
use miku_markdown::{extract_title, is_asset_path, normalize_target, TAG_REGEX};
use regex::Regex;
use serde_json::Value;
use std::path::Path;

mod mentions;

pub use mentions::extract_mentions;

static WIKILINK_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(!?)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("wikilink regex")
});

/// Build one complete index projection from a Markdown file's bytes.
pub fn build_page_index(path: &str, raw: &[u8], mtime: i64) -> PageIndex {
    let content = String::from_utf8_lossy(raw).replace('\0', "");
    let (frontmatter, body) = miku_markdown::parse_frontmatter(&content);
    let frontmatter = frontmatter.unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let title = extract_title(path, Some(&frontmatter), body);
    let headings = miku_markdown::extract_headings(body)
        .into_iter()
        .map(|heading| HeadingSummary {
            level: heading.level,
            text: heading.text,
        })
        .collect();

    let links = WIKILINK_REGEX
        .captures_iter(body)
        .map(|capture| {
            let target = capture[2].trim().to_string();
            let is_embed = !capture[1].is_empty();
            LinkRecord {
                target: target.clone(),
                target_norm: normalize_target(&target, is_embed && is_asset_path(&target)),
                alias: capture
                    .get(3)
                    .map(|value| value.as_str().trim().to_string()),
                kind: if is_embed && is_asset_path(&target) {
                    LinkKind::Asset
                } else {
                    LinkKind::Page
                },
                is_embed,
            }
        })
        .collect();

    let mut tags = frontmatter_tags(&frontmatter);
    tags.extend(
        TAG_REGEX
            .captures_iter(body)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string())),
    );
    tags.sort();
    tags.dedup();

    let mut aliases = frontmatter_aliases(&frontmatter);
    aliases.sort();
    aliases.dedup();

    PageIndex {
        summary: PageSummary {
            path: path.to_string(),
            title,
            frontmatter,
            mtime,
        },
        body: body.to_string(),
        links,
        tags,
        aliases,
        has_mermaid: body.lines().any(|line| line.trim() == "```mermaid"),
        signals: DocumentSignals {
            lead: miku_markdown::extract_lead(body),
            headings,
            word_count: body.split_whitespace().count(),
        },
    }
}

fn frontmatter_tags(frontmatter: &Value) -> Vec<String> {
    match frontmatter.get("tags") {
        Some(Value::String(tag)) => vec![tag.trim_start_matches('#').to_string()],
        Some(Value::Array(tags)) => tags
            .iter()
            .filter_map(Value::as_str)
            .map(|tag| tag.trim_start_matches('#').to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn frontmatter_aliases(frontmatter: &Value) -> Vec<String> {
    match frontmatter.get("aliases") {
        Some(Value::String(alias)) => vec![alias.to_string()],
        Some(Value::Array(aliases)) => aliases
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Return the normalized basename used by the default page resolver.
pub fn page_slug(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_lowercase)
        .unwrap_or_else(|| path.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_page_projection_from_markdown() {
        let page = build_page_index(
            "Notes/Today.md",
            b"---\ntags: [daily]\naliases: [Now]\n---\n# Today\n\n[[Index]] #journal",
            42,
        );

        assert_eq!(page.summary.title, "Today");
        assert_eq!(page.tags, vec!["daily", "journal"]);
        assert_eq!(page.aliases, vec!["Now"]);
        assert_eq!(page.links[0].target_norm, "index");
    }

    #[test]
    fn strips_nuls_and_detects_mermaid() {
        let page = build_page_index("Diagram.md", b"# D\0\n```mermaid\ngraph TD\n```", 1);

        assert!(!page.body.contains('\0'));
        assert!(page.has_mermaid);
    }
}
