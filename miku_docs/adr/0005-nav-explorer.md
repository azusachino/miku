---
id: ADR-0005
title: Navigation explorer (folder/file tree)
slug: nav-explorer
status: Accepted
date-proposed: 2026-06-26
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:nav-explorer
supersedes: []
superseded-by:
relates-to: [ADR-0002]
depends-on: []
impacts: [src/handlers/tree, src/render]
new-schema: false
tags: [frontend, navigation, server-rendered, no-js]
---

# ADR-0005 — Navigation explorer (folder/file tree)

## Decision

A Quartz-style collapsible **explorer** in the sidebar, rendered **server-side** from `tb_pages.path` using native `<details>`/`<summary>` — **zero client JS, no schema change**. Folders are derived
**path-prefixes**, not rows; the tree is a pure function of the existing index, so the read path stays **Postgres-only** (consistent with the single-writer, read-only-handler model).

## Why

Fits the decided frontend stance (`product.md` → _"no JS bundler / server-first" ≠ "zero JS"_): Mermaid is the **one** deliberate client-JS exception; everything else stays server-rendered.
`<details>` gives collapse/expand for free, works with JS disabled, no bundler. `tb_pages.path` already encodes the hierarchy.

**Physical folders, not tags.** The explorer's job is **spatial orientation** ("where does this file live"), which folders answer directly. A tag-hierarchy tree (`#area/sub`) is a _different_ view and
can ship later at `/tags` reusing the same `<details>` renderer over `tb_tags`. Folders stay the "loose filing cabinet"; the explorer makes it navigable.

**Scale.** Full server-render for typical vaults; past a threshold, render top levels collapsed and **lazy-load** children on expand from `/tree?prefix=…`. Never emit 100k nodes eagerly. Active page's
ancestor folders get `open`.

**Ordering.** Folders-before-files, alphabetical (free from a `BTreeMap` walk). Frontmatter `order`/`nav` hints deferred.

## Trade-offs / Rejected

(1) A client-side JS tree widget — reintroduces the bundler/Electron tax ADR-0002 and `product.md` exist to avoid. (2) A `tb_folders` table — folders are derivable from `path`; materializing them
duplicates truth and invites drift.
