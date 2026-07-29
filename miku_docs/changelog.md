---
title: Changelog
type: changelog
status: active
tags: [miku, release]
updated: 2026-07-29
---

# Changelog

User-facing changes to Miku Note are recorded here. See [[index|Miku Note]] for the starting point and [[features|Features]] for the current product boundary. #release

## v0.0.5 — workspace consistency and SQLite-only search (2026-07-29)

- Removed Tantivy and SQLite FTS5; SQLite now stores one body copy and performs parallel plain-content search, while MemoryIndex retains only graph metadata.
- Added backend-resolved outgoing links, ignored links inside inline/fenced code, and corrected lazy tree parent/expansion behavior.
- Added an accessible mobile navigation drawer, calmer dark scrollbars, theme-aware favicon behavior, and curated real frontmatter properties.
- Fixed active tabs reopening during close navigation and cleared quick-search input after selecting a note.
- Reduced folder tree reads to one representative row per immediate child and kept folder-scoped parent IDs consistent.
- Replaced client-only tag slicing with bounded `/api/v1/tags` pagination and incremental page fetching.
- Folded the former workspace-cleanup unreleased notes into the shipped v0.0.3 record below.

## v0.0.4 — note save performance & trilium editor layout (2026-07-28)

- Optimized `PUT /api/v1/notes/{id}` save performance from ~11.45s down to <5ms via $O(1)$ single-term Tantivy index updates and multi-key LRU document caching.
- Optimized file system watcher to avoid triggering full vault reconcile sweeps on single-file atomic Markdown saves.
- Added $O(1)$ fast-path extensionless note resolution in `resolve_document` to prevent full vault scanning on extensionless note path requests.
- Optimized reconcile process to bypass re-reading and re-parsing 14,000+ unchanged files when the in-memory index is already ready.
- Fixed editor layout and typography alignment in CodeMirror to respect Trilium-style measure without hidden horizontal scrollbars or margin offsets.
- Added instant optimistic frontend cache update (`queryClient.setQueryData`) upon save completion for zero-delay reading mode rendering.

## v0.0.3 — file-based workspace (2026-07-16)

- Added the persistent React workspace shell with a lazy file tree, scrollable tabs, breadcrumbs, quick search, tags, backlinks, context panels, and theme switching.
- Added opt-in Markdown Source mode with CodeMirror, optimistic revision checks, direct path saves, and frontmatter-preserving body writes.
- Added extensionless `/p/xxx` route normalization to `/p/xxx.md` and broader Prism/Refractor language support for modern documentation.
- Added Mermaid, GitHub-style alerts, KaTeX math, table-of-contents navigation, lazy tags, and folder/file browsing to the reader surface.
- Fixed language-less and highlighted code blocks so their contrast, padding, scrolling, and theme colors remain readable in both light and dark mode.
- Split the Rust workspace into domain, vault, Markdown, index, cache, application, and HTTP layers with SQLite as the default durable projection.
- Replaced the former server-rendered/Alpine frontend description with the current React, Vite, Tailwind, Prism, Mermaid, and KaTeX architecture.
- Organized the frontend source by app, workspace, Markdown, components, and shared helpers.
- Removed duplicate planning notes and stale pre-workspace documentation.
- Standardized first-party Markdown notes and ADRs on Obsidian-style YAML frontmatter.
- Renamed the HTTP handler module to http_api to distinguish transport code from workspace domain behavior.

## v0.0.2 — Miku Note reader and frontend refresh (2026-07-15)

### Reader performance

- Page links swap the active reader inside the persistent React shell; navigating between notes does not reload shared CSS or JavaScript.
- Reader mode no longer opens an idle `/events` stream. The active page checks for freshness periodically and when the tab becomes visible again.
- CodeMirror, Prism, Mermaid, and KaTeX are loaded only when the current page needs them.
- Replaced the separate indexed-search and content-search page modes with one search model: Pages, Content, and Commands are tabs in the Cmd-K palette.
- Search is now a single quick-search panel over title, content, and combined projections; it is opened from the persistent shell with Cmd-K.

### Reading experience

- Rebranded the visible frontend as **Miku Note**.
- Added Thin, Wide, and Full reading-width modes with persisted preferences.
- Kept the right reading rail beside the article in Thin and Wide modes; it collapses only at the responsive breakpoint or in Full mode.
- Simplified breadcrumbs and hid the internal `miku_docs/` root from user-facing labels.
- Added scroll-triggered paging for `/tags`; there is no visible “Load more” button.

### Markdown rendering

- Added lazy Mermaid rendering with diagram zoom.
- Added lazy Prism highlighting and code-block copy actions.
- Added dollar math parsing and lazy KaTeX rendering for inline `$...$` and display `$$...$$` equations.
- Updated [[sandbox|Sandbox]] with Mermaid, code, and math fixtures for browser acceptance checks.

### Scope clarification

- The content root is `miku_docs/`.
- The default local index is SQLite via SQLx; the supported Postgres profile remains available for the scale/container path.
- The browser editor is CodeMirror-based and opt-in from the reader; ordinary reading leaves its modules unloaded.

### Brand language

- Adapted the canonical light/dark Miku icon from the design preview for the shell and favicon.
- Reused the same mark in empty search and tag states as a small Markdown-native brand cue; ordinary note content stays quiet and readable.

## v0.0.1 — MVP (2026-06-26)

The first release established the filesystem-owned Markdown wiki, atomic saves, background indexing, wikilinks, backlinks, tags, full-text search, and a basic browser editor. The current reader
refresh above supersedes its original limitations and UI descriptions.
