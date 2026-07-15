# Changelog

User-facing changes to Miku Note are recorded here. See [[Index]] for the
starting point and [[Features]] for the current product boundary. #release

## v0.0.2 — Miku Note reader and frontend refresh (2026-07-15)

### Reader performance

- Page links now swap a server-rendered reader fragment inside the persistent
  shell; navigating between notes does not reload the document, shared CSS, or
  shared JavaScript.
- Reader mode no longer opens an idle `/events` stream. The active page checks
  for freshness periodically and when the tab becomes visible again.
- CodeMirror, Prism, Mermaid, and KaTeX are loaded only when the current page
  needs them.
- Replaced the separate indexed-search and content-search page modes with one
  search model: Pages, Content, and Commands are tabs in the Cmd-K palette.
- `/search` is now the full Markdown content-search page; embedded ripgrep is
  the body-search source of truth, while the disposable index remains an
  internal navigation and relationship accelerator.

### Reading experience

- Rebranded the visible frontend as **Miku Note**.
- Added Thin, Wide, and Full reading-width modes with persisted preferences.
- Kept the right reading rail beside the article in Thin and Wide modes; it
  collapses only at the responsive breakpoint or in Full mode.
- Simplified breadcrumbs and hid the internal `miku_docs/` root from user-facing
  labels.
- Added scroll-triggered paging for `/tags` and `/tags/<tag>`; there is no
  visible “Load more” button.

### Markdown rendering

- Added lazy Mermaid rendering with diagram zoom.
- Added lazy Prism highlighting and code-block copy actions.
- Added dollar math parsing and lazy KaTeX rendering for inline `$...$` and
  display `$$...$$` equations.
- Updated [[Sandbox]] with Mermaid, code, and math fixtures for browser
  acceptance checks.

### Scope clarification

- The content root is `miku_docs/`, not `miku/`.
- The default local index is Turso; the supported Postgres profile remains
  available for the scale/container path.
- The browser editor is CodeMirror-based and opt-in from the reader; it is not
  loaded during ordinary reading.

### Brand language

- Adapted the canonical light/dark Miku icon from the design preview for the
  shell and favicon.
- Reused the same mark in empty search and tag states as a small
  Markdown-native brand cue; ordinary note content stays quiet and readable.

## v0.0.1 — MVP (2026-06-26)

The first release established the filesystem-owned Markdown wiki, atomic saves,
background indexing, wikilinks, backlinks, tags, full-text search, and a basic
browser editor. The current reader refresh above supersedes its original
limitations and UI descriptions.
