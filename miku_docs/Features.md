# Miku Note Features

Miku Note is a filesystem-owned Markdown wiki with a thin browser reader and a
rebuildable background index. This page lists what is available now; deferred
ideas are kept at the end so the product boundary stays honest. #feature #guide

> [!NOTE]
> Markdown files under `miku_docs/` are the source of truth. The index is
> disposable and can be rebuilt from those files.

## Filesystem ownership

Notes remain ordinary `.md` files under `miku_docs/`. They can be edited with
Miku Note, a text editor, scripts, or git. The database is an index projection;
Markdown files remain the canonical copy of your content.

## Reader-first navigation

The readonly reader is the primary experience. Direct page URLs use `/p/...`,
so links, bookmarks, and server-rendered requests continue to work normally.
Switching between notes keeps the application shell mounted and swaps only the
reader fragment; the document, shared JavaScript, and CSS remain loaded.

The reader includes:

- a collapsible filesystem explorer;
- breadcrumbs, backlinks, unlinked mentions, and page properties;
- Thin, Wide, and Full reading-width modes;
- light/dark themes, accent colors, compact spacing, and Zen mode;
- a quick switcher and command palette (`Ctrl-K` / `Ctrl-Shift-P`).

## Markdown and rich rendering

The Rust renderer supports CommonMark/GFM features including tables,
strikethrough, task lists, autolinks, footnotes, alerts, and raw HTML for
trusted local files.

Miku Note also supports:

- `[[Page]]` wikilinks, aliases, and missing-link styling;
- `![[asset.png]]` asset embeds;
- inline `#tags` and YAML frontmatter properties;
- fenced Mermaid diagrams;
- fenced code blocks with language-aware Prism highlighting;
- inline and display LaTeX-style math (`$...$` and `$$...$$`).

Mermaid, Prism, and KaTeX are loaded only when the current reader content needs
them. Code blocks receive a small copy action on demand. These enhancements are
progressive: the server-rendered Markdown remains the source of the page.

## Wikilinks, backlinks, and mentions

Write `[[PageName]]` to connect notes. Matching is case-insensitive, and links
can include a display alias such as `[[Index|Home]]`.

When a note links to another note, the target page shows explicit backlinks.
The indexer also records plain-text mentions as a secondary discovery surface;
an unlinked mention can be promoted to a real wikilink from the reader.

## Tags and search

Use tags such as `#docs`, `#feature`, or `#area/sub` anywhere in prose. Tags
from Markdown and frontmatter are indexed together.

- `/tags` shows the tag index and loads more results as you scroll;
- `/tags/<tag>` shows matching pages and also uses scroll-triggered paging;
- `/search` searches Markdown source content directly with embedded ripgrep;
  results are grouped by note and loaded as you scroll;
- `Cmd-K` opens one palette with Pages, Content, and Commands tabs;
- Pages uses the disposable title/path index for fast switching, while Content
  uses the Markdown files as the search source of truth.

## Editing and safe writes

Editing is opt-in from the reader. The inline editor uses CodeMirror 6 and
loads its editor modules only when editing starts; the full editor remains
available at `/p/<path>/edit`.

Saving writes a temporary file, flushes it, and atomically renames it into
place. A changed-file hash protects against overwriting edits made elsewhere.

## Background indexing

The filesystem watcher is the sole live index trigger. After a file changes, it
updates links, tags, aliases, mentions, full-text search data, and rendering
metadata for that page. HTTP handlers read the index and do not perform an
inline reindex.

The index is rebuildable from `miku_docs/**/*.md`. The local runtime uses
SQLite via SQLx by default; larger deployments can select the supported
Postgres profile. The index accelerates
navigation and relationships; body search remains the dedicated full-text mode.

## File ownership boundary

The file tree is read-only in v0.0.2. Miku does not move, rename, delete, or
trash pages. Use the editor for content changes and use your filesystem, editor,
scripts, or git when changing paths or removing files; the watcher reconciles
those external changes into the disposable index.

## Freshness and external edits

Miku Note keeps the event stream active only while the editor is open. The
active page checks for a newer indexed version periodically and refreshes when
the tab becomes visible again. This keeps reading lightweight while still reflecting
changes made by git, an editor, or another process.

## Deliberately deferred or rejected

The following items remain outside the current feature set:

- mobile or offline-first applications;
- real-time collaboration and CRDT editing;
- built-in encryption or cloud sync;
- a Notion-style database/block model;
- a plugin runtime or general client-side application framework;
- graph/canvas view;
- drag-and-drop asset upload;
- Dataview-style queries and daily-note/calendar workflows;
- MDX/JSX as a Markdown rendering requirement.

For a hands-on compatibility page, see [[Sandbox]]. For setup instructions,
see [[Usage]].
