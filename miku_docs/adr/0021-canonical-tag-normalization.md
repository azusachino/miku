---
id: ADR-0021
type: adr
title: ADR-0021 — Canonical Tag Normalization (#tag)
slug: canonical-tag-normalization
status: Accepted
updated: 2026-07-29
date-proposed: 2026-07-29
date-accepted: 2026-07-29
deciders: [haru]
mirror: asobi:miku:decision:canonical-tag-normalization
supersedes: []
superseded-by: []
relates-to: [ADR-0017, ADR-0019]
impacts: [crates/miku-markdown, crates/miku-indexer, crates/miku-index-memory, crates/miku-index-sqlite, crates/miku/src/http_api.rs, miku-web]
tags: [tags, normalization, index, domain]
---

## ADR-0021 — Canonical Tag Normalization (#tag)

### Decision

Enforce canonical **lowercased, hyphen-connected** tag normalization (`normalize_tag`) across all layers of the system:

1. Markdown inline tag extraction (`#tag`).
2. Frontmatter `tags` list parsing.
3. In-memory and SQLite index tag storage and query lookups (`pages_with_tag`).
4. HTTP REST API endpoints (`/api/v1/tags`, `/api/v1/tags/{tag}/notes`).
5. HTML link rendering (`<a href="/tags/tag-name">#tag-name</a>`).

Canonical normalization rules:

* Strip leading `#` and trim whitespace.
* Convert all characters to lowercase.
* Replace spaces, underscores, and consecutive hyphens with a single hyphen (`-`).
* Strip leading/trailing hyphens.

Example conversions:

* `#Todo` $\rightarrow$ `"todo"`
* `#my_tag` $\rightarrow$ `"my-tag"`
* `#MY TAG` $\rightarrow$ `"my-tag"`
* `#TO-DO` $\rightarrow$ `"to-do"`

### Why

* **Eliminate Tag Fragmentation**: Raw tags like `#Todo`, `#todo`, `#my_tag`, and `#MY TAG` previously produced fragmented, duplicate tag entries in the index and UI.
* **Predictable Navigation**: Users clicking inline `#tag` links expect consistent filtering regardless of letter casing or separator style used in individual notes.

### Trade-offs / Rejected

* **Rejected**: Preserving exact case sensitive tags in the index. Case-sensitive matching breaks search, causes index fragmentation, and creates duplicate tag pills in the navigation tree.
* **Preserved**: Frontmatter original text remains untouched in source `.md` files; normalization applies strictly to index keys, URL routes, and tag queries.
