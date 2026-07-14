use aho_corasick::{AhoCorasickBuilder, MatchKind};
use miku_domain::{MentionRecord, PageIndex};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static EXCLUDED_RANGES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)```.*?```|~~~.*?~~~|`[^`\n]*`|!?\[\[[^\]]+\]\]|!?\[[^\]]+\]\([^\)]*\)",
    )
    .expect("mention exclusion regex")
});

#[derive(Debug, Clone)]
struct Candidate {
    target_path: String,
    source_title: String,
    matched_text: String,
}

/// Extract exact title/alias mentions from one already-parsed page.
///
/// The matcher scans the body once, then applies Markdown-aware exclusions.
/// Ambiguous titles and existing explicit links are intentionally ignored.
pub fn extract_mentions(source: &PageIndex, pages: &[PageIndex]) -> Vec<MentionRecord> {
    let mut patterns = Vec::new();
    let mut candidates = Vec::new();
    let mut owners = HashMap::<String, Option<String>>::new();

    for target in pages {
        if target.summary.path == source.summary.path {
            continue;
        }
        let names = std::iter::once(target.summary.title.as_str())
            .chain(target.aliases.iter().map(String::as_str));
        for name in names {
            let normalized = name.trim().to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            let owner = owners.entry(normalized.clone()).or_insert(None);
            if owner.is_none() {
                *owner = Some(target.summary.path.clone());
            } else if owner.as_deref() != Some(target.summary.path.as_str()) {
                *owner = Some(String::new());
            }
        }
    }

    for target in pages {
        if target.summary.path == source.summary.path {
            continue;
        }
        let names = std::iter::once(target.summary.title.as_str())
            .chain(target.aliases.iter().map(String::as_str));
        for name in names {
            let normalized = name.trim().to_lowercase();
            if normalized.is_empty() || owners.get(&normalized).and_then(Option::as_deref) == Some("")
            {
                continue;
            }
            patterns.push(name.trim().to_string());
            candidates.push(Candidate {
                target_path: target.summary.path.clone(),
                source_title: source.summary.title.clone(),
                matched_text: name.trim().to_string(),
            });
        }
    }

    if patterns.is_empty() || source.body.is_empty() {
        return Vec::new();
    }
    let Ok(matcher) = AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostLongest)
        .ascii_case_insensitive(true)
        .build(&patterns)
    else {
        return Vec::new();
    };

    let linked_targets = source
        .links
        .iter()
        .map(|link| link.target_norm.as_str())
        .collect::<HashSet<_>>();
    let excluded = EXCLUDED_RANGES
        .find_iter(&source.body)
        .map(|matched| matched.start()..matched.end())
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut mentions = Vec::new();

    for matched in matcher.find_iter(&source.body) {
        if excluded
            .iter()
            .any(|range| range.start <= matched.start() && matched.end() <= range.end)
            || !word_boundary(&source.body, matched.start(), matched.end())
        {
            continue;
        }
        let candidate = &candidates[matched.pattern().as_usize()];
        if linked_targets.contains(miku_indexer_slug(&candidate.target_path).as_str())
            || !seen.insert((candidate.target_path.clone(), candidate.matched_text.to_lowercase()))
        {
            continue;
        }
        mentions.push(MentionRecord {
            target_path: candidate.target_path.clone(),
            source_path: source.summary.path.clone(),
            source_title: candidate.source_title.clone(),
            matched_text: source.body[matched.start()..matched.end()].to_string(),
            snippet: snippet(&source.body, matched.start(), matched.end()),
        });
    }
    mentions.sort_by(|left, right| {
        left.target_path
            .cmp(&right.target_path)
            .then(left.matched_text.cmp(&right.matched_text))
    });
    mentions
}

fn miku_indexer_slug(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_lowercase)
        .unwrap_or_else(|| path.to_lowercase())
}

fn word_boundary(body: &str, start: usize, end: usize) -> bool {
    fn word(value: Option<char>) -> bool {
        value.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
    }
    let before = body[..start].chars().next_back();
    let after = body[end..].chars().next();
    !word(before) && !word(after)
}

fn snippet(body: &str, start: usize, end: usize) -> String {
    let left = body[..start]
        .char_indices()
        .rev()
        .nth(64)
        .map_or(0, |(index, _)| index);
    let right = body[end..]
        .char_indices()
        .nth(96)
        .map_or(body.len(), |(index, _)| end + index);
    let mut result = body[left..right].split_whitespace().collect::<Vec<_>>().join(" ");
    if left > 0 {
        result.insert_str(0, "… ");
    }
    if right < body.len() {
        result.push_str(" …");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{DocumentSignals, PageSummary};

    fn page(path: &str, title: &str, body: &str) -> PageIndex {
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: title.to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
            },
            body: body.to_string(),
            links: Vec::new(),
            tags: Vec::new(),
            aliases: Vec::new(),
            has_mermaid: false,
            signals: DocumentSignals::default(),
        }
    }

    #[test]
    fn extracts_plain_mentions_and_skips_markdown_regions() {
        let mut source = page(
            "Source.md",
            "Source",
            "Index is useful. [[Index]] `Index` [Index](https://example.com)\n```\nIndex\n```",
        );
        let target = page("Index.md", "Index", "target");
        let mentions = extract_mentions(&source, &[source.clone(), target.clone()]);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].matched_text, "Index");

        source.links.push(miku_domain::LinkRecord {
            target: "Index".to_string(),
            target_norm: "index".to_string(),
            alias: None,
            kind: miku_domain::LinkKind::Page,
            is_embed: false,
        });
        assert!(extract_mentions(&source, &[source.clone(), target]).is_empty());
    }

    #[test]
    fn skips_ambiguous_titles_and_substrings() {
        let source = page("Source.md", "Source", "Index indexing Index");
        let first = page("one/Index.md", "Index", "target");
        let second = page("two/Index.md", "Index", "target");
        assert!(extract_mentions(&source, &[source.clone(), first, second]).is_empty());

        let target = page("Index.md", "Index", "target");
        let mentions = extract_mentions(&source, &[source.clone(), target]);
        assert_eq!(mentions.len(), 1);
    }
}
