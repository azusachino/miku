---
id: ADR-0002
type: adr
title: ADR-0002 — Markdown & wikilink grammar (comrak)
slug: markdown-wikilink-grammar
status: Accepted
updated: 2026-06-26
date-proposed: 2026-06-25
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:markdown-wikilink-grammar
supersedes: []
superseded-by:
relates-to: [ADR-0005]
embeds-decision: miku:decision:no-mdx
impacts: [src/render, src/indexer]
tags: [markdown, comrak, wikilinks, tags, callouts]
---

# ADR-0002 — Markdown & wikilink grammar (comrak)

## Decision

**Renderer — `comrak`** (Rust cmark-gfm). Built-in extension options provide GFM (tables, strikethrough, task lists, footnotes, autolinks), **native wikilinks** (`[[Target|Display]]`), and **native
GitHub alerts** (`> [!NOTE]` callouts). Only two custom extractors remain — `#tags` and `![[embed]]`.

**Tag grammar.** `#tag` = Unicode letters/digits plus `_ - /` (hierarchical `#area/health`). Recognized only in text (not code, URL fragments, ATX headings); stored whole, ancestor grouping is a
**query-time prefix match**.

**Callouts.** Obsidian/GitHub `> [!type]` wins (native via comrak alerts); `:::` directive syntax dropped. **Transclusion** `![[Page]]` / `![[image.png]]` is custom + server-side, with a
recursion/cycle depth limit.

## Why

Keeps custom grammar work to the two things comrak does not provide natively. Render speed is acceptable for a personal wiki; rendered HTML can be cached by content hash later.

## Trade-offs / Rejected

No MDX/JSX — needs a JS runtime, breaks plain-Markdown portability (`miku:decision:no-mdx`). CM6 is a _separate axis_: a client-side editor that does not render the read view or feed the index (server
still parses via comrak) — roadmap, orthogonal to this ADR.
