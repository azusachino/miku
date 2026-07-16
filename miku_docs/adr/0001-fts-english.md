---
id: ADR-0001
type: adr
title: ADR-0001 — Full-text search (Postgres english FTS)
slug: fts-english
status: Accepted
updated: 2026-06-26
date-proposed: 2026-06-25
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:fts-english
supersedes: []
superseded-by:
relates-to: []
impacts: [migrations/0001_init_index.sql, src/indexer]
tags: [search, postgres, index]
---

# ADR-0001 — Full-text search (Postgres english FTS)

## Decision

Use Postgres' built-in **`english`** FTS config: `to_tsvector('english', title ‖ body)` with **title weighted A, body B**, ranked by `ts_rank`, snippets via `ts_headline('english', …)`.

## Why

No extension, no Rust tokenizer, no spike. Postgres stays **vanilla**, so the index is disposable/portable and identical across compose and k8s. Content and titles are English in practice; the app
name (ミク) is branding, not a content-language requirement. This was the only ADR that needed a spike — it is eliminated.

## Trade-offs / Rejected

Deferred, not rejected: if meaningful CJK content ever shows up, revisit with app-side
`lindera` tokenization or `pgroonga`. Switching is just a reindex — cheap, because the
index is fully rebuildable.
from files.
