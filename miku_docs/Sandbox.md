# Sandbox: Markdown & Wikilink Playground

Welcome to the Sandbox! This page demonstrates Miku's Markdown support and wikilink behavior. #demo

## Markdown Syntax

### GitHub Alerts

> [!NOTE]
> This is a note alert. Miku renders GitHub alert syntax (`[!NOTE]`, `[!WARNING]`, `[!IMPORTANT]`) natively using comrak.

> [!WARNING]
> This is a warning. Use it to highlight potential pitfalls or precautions.

### Code Blocks

Here's a Rust snippet showing how pages are indexed:

```rust
fn extract_wikilinks(markdown: &str) -> Vec<String> {
    // Simple regex-based wikilink extraction
    regex::Regex::new(r"\[\[([^\]]+)\]\]")
        .unwrap()
        .captures_iter(markdown)
        .map(|cap| cap[1].to_string())
        .collect()
}
```

### Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Wikilinks | Stable | Bi-directional backlinks included |
| Full-Text Search | Stable | Postgres FTS with ranking |
| Tag Index | Stable | Hashtag-based filtering |
| Atomic Saves | Stable | Guarantees consistency |
| Background Indexer | Stable | Single-writer, incremental updates |

## Wikilinks Showcase

Miku uses wikilinks to connect your wiki into a network. Try clicking these links:

- [[Index]] — the main landing page and entry point to the wiki.
- [[Features]] — a detailed walkthrough of each Miku capability.
- [[Usage]] — how to set up and run Miku locally.
- [[Changelog]] — release notes and version history.

You can also use link text overrides: [[Index|Back to Home]] displays as "Back to Home" but links to [[Index]].

## Tags in Context

This page uses the #demo tag to mark it as a playground for new users. Other pages use #feature, #guide, #docs, and #release to organize content by type. Browse the tag index to see how pages cluster. #demo

The [[Features]] page discusses tags in detail — how they're extracted, indexed, and used to filter and explore your wiki.

## What Next?

Explore the wiki:
1. Click through the wikilinks above to see backlinks in action.
2. Use full-text search (top of the page) to find phrases like "atomic saves" or "background indexer".
3. Browse the [[Index]] to see the wiki structure.
4. Read [[Features]] to understand how wikilinks, tags, and backlinks work behind the scenes.

For hands-on setup instructions, see [[Usage]]. For historical context and version information, see [[Changelog]].

---

Happy wiki writing!
