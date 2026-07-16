---
title: Miku Architecture
type: architecture
status: active
tags: [miku, architecture, rust, markdown]
updated: 2026-07-16
---

# Miku Architecture

Miku is a filesystem-owned Markdown workspace. The files under miku_docs/ are the product data; Rust services and indexes make those files navigable without creating a second source of truth.

## Repository shape

```text
repo/
├── crates/
│   ├── miku/                    # binary and HTTP transport
│   ├── miku-app/                # application services and composition
│   ├── miku-domain/             # backend-neutral contracts and records
│   ├── miku-vault/              # safe Markdown filesystem adapter
│   ├── miku-markdown/           # Markdown parsing and source transforms
│   ├── miku-indexer/            # filesystem-to-index projection builder
│   ├── miku-index-memory/       # hot graph and Tantivy search projection
│   ├── miku-index-sqlite/       # local durable projection
│   ├── miku-index-postgres/     # optional scale projection
│   └── miku-cache-valkey/       # optional best-effort cache
├── miku-web/                    # React, TypeScript, Vite frontend
└── miku_docs/                   # authoritative Markdown vault
```

## Source and projection boundary

miku_docs/\*_/_.md and user assets are authoritative. miku-vault owns safe path normalization, atomic file operations, Markdown frontmatter, revisions, and scans. It does not own search or backlinks.

miku-domain defines the stable vocabulary shared by the vault, indexer, application, and projection backends: notes, links, tags, revisions, search requests, and index capabilities. It intentionally
contains no filesystem or database implementation.

The index is disposable. The default runtime composes a local SQLite durable projection with an in-memory/Tantivy hot projection. Postgres and Valkey are optional scale components. Rebuilding any
projection from miku_docs/ must recover the same searchable relationships.

## Runtime flow

1. miku-vault reads the requested Markdown source.
2. miku-app composes vault access, workspace policy, and index readers.
3. miku exposes the application through the versioned JSON API.
4. The background indexer watches folders, debounces filesystem events, parses changed notes, and updates projections.
5. miku-web keeps route, tabs, tree, search, and reader state in the browser.

HTTP handlers read projections and source documents. They do not synchronously rebuild indexes. A save writes atomically; the watcher schedules reconciliation. Startup reconciliation catches changes
missed while the process was stopped.

## Frontend boundary

The browser frontend is a separate Vite project. Its structure follows features rather than delivery history:

- src/app/ owns route composition.
- src/features/workspace/ owns workspace state, API clients, and the shell.
- src/features/markdown/ owns editor and reader integrations.
- src/components/workspace/ owns reusable tree, notice, and icon components.
- src/shared/ owns cross-feature UI state and pure helpers.

Tailwind provides shell utilities and tokens; Tailwind Typography owns generic Markdown typography; React Markdown plus Prism, Mermaid, and KaTeX provide the rendering pipeline. Miku-specific CSS is
limited to interaction behavior, alerts, links, diagrams, and shell details.

## Link and metadata model

Obsidian-style wikilinks, Markdown links, aliases, embeds, tags, and unlinked mentions are parsed from source Markdown. Explicit /p/<path>.md links remove ambiguity; unique basename wikilinks remain
convenient. Backlinks are derived index edges and never require scanning candidate files during a page request.

Every first-party note uses YAML frontmatter for stable metadata. The minimum convention is title, type, status, tags, and updated; ADRs also carry an immutable id.
