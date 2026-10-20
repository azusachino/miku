---
id: ADR-0023
type: adr
title: ADR-0023 — Product definition: a reader-first wiki for a large Markdown library
slug: reader-first-library-wiki
status: Accepted
updated: 2026-09-30
date-proposed: 2026-09-30
date-accepted: 2026-09-30
deciders: [haru]
mirror: asobi:miku:decision:reader-first-library-wiki
supersedes: [ADR-0012]
superseded-by: []
relates-to: [ADR-0017, ADR-0019, ADR-0020, ADR-0022]
impacts: [miku_docs/product.md, README.md, crates/miku-index-postgres, crates/miku-cache-valkey, miku-web]
tags: [product, scope, positioning]
---

## ADR-0023 — Product definition: a reader-first wiki for a large Markdown library

### Decision

**Miku is a self-hosted, reader-first wiki for a large Markdown library.** It turns a folder of Markdown files into a fast, linked, searchable website, and that folder stays the only source of truth.

The finished product is a website for your Markdown folder. You read it, search it, and follow its links, while any other editor (Obsidian, a text editor, git, scripts) keeps working on the same files. Miku works alongside those tools; it does not replace them.

This ADR is the scope filter for every later decision. A proposal that serves a non-goal below needs a new ADR that supersedes this one.

### Goals

1. **Fast reading at library scale.** Opening a page, moving between pages, and loading its context stay fast at 10k–100k files. Per-request work must not grow with vault size.
2. **Search across the whole library,** including CJK text.
3. **A computed link graph.** Links, backlinks, aliases, and tags are derived in the background from files that anyone may edit.
4. **A provable rebuild.** Dropping every projection and rebuilding from the files yields the same answers, and a test proves it.
5. **One binary plus a folder.** Runs on localhost by default and on a trusted network when explicitly configured.
6. **Safe, sufficient editing.** Atomic writes, revision conflict detection, and quick in-place fixes. Editing is a supporting flow, not the primary one.

### Non-goals

- **A daily-capture editor** that competes with Obsidian or SilverBullet. Those tools can edit the same folder.
- **A Trilium-style hierarchical workspace.** No clones, placements, hoisting, or split panes. The folder structure is the navigation tree.
- **Extensibility runtimes.** No plugins, scripting, or query language.
- **Multi-user, hosted, or real-time collaborative use.** Single user, single writer.
- **Sync, mobile apps, or built-in encryption.** These belong to git and the filesystem.

### Consequences for existing decisions

- **ADR-0012 (Postgres and Valkey scale tier) is superseded.** Those backends no longer serve a product goal. Their crates remain and keep compiling under `make check-all-features` until a separate change removes them. They receive no new features.
- **ADR-0017 is partly superseded.** Still in force: the React frontend boundary, domain-oriented HTTP APIs, and the file-based note graph. Replaced: the "Trilium-like workspace" identity and the frontend scope of tabs and splits, hoisting, bookmarks, and clone placements.
- **Frontmatter `parents` is frozen.** Existing files remain readable and no new placement features are built. Whether it is removed or kept as a plain "up" link is decided in its own ADR.
- **Priorities follow the goals.** Order: constant-time note context (goal 1), the rebuild conformance test (goal 4), CJK search (goal 2), then safe defaults (goals 5 and 6).

### Why

The product documents described three different products: an Obsidian-style linking tool (`product.md`), a Trilium-like workspace (ADR-0017), and a reader-first wiki (`features.md`, README). Each pulls the architecture in a different direction. Carrying all three produced parallel backends and a placement model that plain folders cannot express. That model is what forces a full-vault scan on every page view.

Miku's evidence points to the reader: the shipped read-only mode and reader-first UI, a server-side Rust index as the strongest component, a 15.9k-file course library as the real vault, and personas (compliance records, investigative research, worldbuilding reference) whose daily payoff is finding and following content.

### Trade-offs / Rejected

- **B. Obsidian in the browser (editor first).** Rejected. The main investment would move to the editor and capture flow, where Obsidian and SilverBullet are already mature, and Miku's server-side index would stop being the differentiator.
- **C. Trilium-like workspace (keep ADR-0017 in full).** Rejected. Clones and placements do not map onto plain folders, so they live in frontmatter and cost a full scan per request. This conflicts with the filesystem-first invariant and goal 1.
- **SilverBullet-style programmable notebook.** Rejected. It depends on client-side indexing, a plugin and query runtime, and a sync engine, all of which `product.md` already rejects. Miku adopts SilverBullet's engineering practices (a unified relation model, backend conformance tests, localhost-by-default binding), not its product shape.
- **Cost accepted:** users who want a rich capture editor must pair Miku with another editor. Users who wanted Trilium-style clones lose that path.
