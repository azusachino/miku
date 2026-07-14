# Changelog

All notable changes to Miku are documented here. See [[Index]] for an overview, and [[Features]] for detailed feature descriptions. #release

## v0.0.1 (2026-06-26)

### MVP: Filesystem-Owned Personal Wiki #release #feature

**Initial release** ships the core vision of Miku: a browser-editable Markdown wiki with background indexing and zero lock-in.

#### Features

- **Wikilinks and Backlinks**: Link pages using `[[PageName]]` syntax; backlinks surface all references automatically. Full support for wikilink text overrides via `[[Target|Display Text]]`.

- **Full-Text Search**: Index all page content with Postgres full-text search. Results are ranked by relevance and updated in the background as pages change.

- **Tag Index**: Extract and filter pages by `#hashtags` embedded in prose. Tag view is automatically generated and updated.

- **Atomic Saves**: Edits are written to temporary files and atomically renamed into place, guaranteeing consistency even under server crashes.

- **Background Indexer**: Single-writer model eliminates races. The indexer watches `miku/` for changes and rebuilds the index incrementally without blocking reads.

- **Browser Editor**: Edit pages directly in your browser. Simple, plain-text Markdown with a focus on content over toolbars.

#### Architecture

- Built in Rust with axum + tokio for the HTTP server.
- Postgres stores a disposable, fully-rebuildable index of pages, links, tags, and full-text content.
- The `miku/` directory contains the source-of-truth Markdown files, version-controlled and owned by you.
- No JavaScript bundler — server-rendered HTML with plain `<textarea>` for editing.

See [[Usage]] for how to run the server, and [[Features]] for a detailed walkthrough of each capability. #docs

#### Known Limitations

- Single-user editing only (concurrent edits may conflict; use atomic saves and version control to manage history).
- Browser editor is minimal (no toolbar, no preview pane — use your editor of choice and refresh).
- Backlinks are not paginated (works well for wikis up to ~10k pages; larger wikis may see performance degradation).

#### Next Steps

Future releases will explore:
- Collaborative editing with conflict resolution.
- Markdown preview and live render pane.
- Full-text search UI enhancements (facets, date filters, relevance tuning).
- Backlink pagination and graph visualization.

---

See [[Changelog]] for historical changes, or [[Index]] to navigate the wiki.
