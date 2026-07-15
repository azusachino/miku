# Quartz-Informed UI/UX Plan

Status: planned for Miku 0.0.2 follow-up work

This document keeps the Quartz reference close to the implementation. It is a design guide, not a requirement to copy Quartz's static-site architecture or add every Quartz feature.

## Reference implementation

- [Quartz authoring content](https://quartz.jzhao.xyz/authoring-content)
- [Quartz frontmatter plugin](https://quartz.jzhao.xyz/plugins/Frontmatter)
- [Quartz layout](https://quartz.jzhao.xyz/layout)
- [Quartz Explorer](https://quartz.jzhao.xyz/features/explorer)
- [Quartz table of contents](https://quartz.jzhao.xyz/features/table-of-contents)
- [Quartz backlinks](https://quartz.jzhao.xyz/features/backlinks)
- [Quartz graph view](https://quartz.jzhao.xyz/features/graph-view)
- [Quartz wikilinks](https://quartz.jzhao.xyz/features/wikilinks)

## Product boundary

Miku is a local Markdown file browser/editor with a persistent index. Quartz is a Markdown publishing system. Miku should adopt Quartz's information hierarchy and content-first visual language, while
retaining Miku's local editing, watcher, and SQLite index. Files remain managed by the user's filesystem; v0.0.2 exposes content editing and page creation only.

Do not copy Quartz's static build pipeline, generated-site assumptions, or graph-first navigation as mandatory application chrome.

## Content and title contract

The page title is resolved deterministically:

```text
frontmatter `title`
    -> filename stem
```

The first H1 is Markdown content. It is available to the table of contents and document outline, but it never becomes page identity implicitly. This matches Quartz's documented `title`-then-filename
behavior and prevents imported code, HTML, and example headings from changing navigation labels.

The title contract must be covered at the Markdown crate boundary and at the HTTP rendering boundary:

- explicit non-empty YAML title wins;
- empty or non-string YAML title falls back to the filename stem;
- H1-only documents use the filename stem;
- headings inside code, HTML, and examples cannot change the title;
- the same title is used by the explorer, search results, tags, breadcrumbs, backlinks, quick switcher, page title, and mention index.

## UX principles to adopt

### 1. Content-first reading surface

The reader is the primary surface. The file explorer is a navigation column, not part of the article's visual centering calculation. The article should be centered within the remaining content region,
with a readable maximum width.

Page metadata, backlinks, and the table of contents stay subordinate in a right rail and never compete with the document title or body.

### 2. Progressive disclosure

- show the table of contents only when there are enough headings to justify it;
- show empty backlinks sections only when backlinks exist;
- keep page metadata compact and collapsible;
- keep descriptions out of the reader header unless explicitly requested by the document metadata;
- reveal secondary actions on focus/hover while preserving keyboard access.

### 3. Stable navigation

The explorer should preserve expanded folders between page changes and restarts. Selection, current path, and title must remain stable while an indexing or page-load request is in flight. A navigation
refresh must not cause icons, editor modules, or unrelated panels to reload.

### 4. Responsive composition

Use three explicit compositions:

- desktop: files panel, reading column, optional rail;
- tablet: files panel or rail yields space to the reader;
- mobile: files panel becomes an overlay, the rail becomes inline or hidden, and the reader gets the full width.

Do not solve smaller layouts by squeezing all columns into a narrower article.

### 5. Long-content resilience

Long filenames, paths, titles, tags, metadata values, tables, code blocks, and hard-wrapped source lines must have deliberate overflow behavior. They should wrap or scroll inside their own region
without widening cards or the page.

## Implementation plan

### Phase 1: title and content identity

1. Remove H1 title inference from the shared Markdown extractor.
2. Keep YAML `title` parsing and filename-stem fallback.
3. Add regression fixtures for the supplied Chinese document and for H1-like text inside raw HTML/code.
4. Verify every consumer uses the indexed title rather than deriving its own label.

Acceptance: a document's H1, code sample, or HTML comment cannot become its page title in the explorer, search, tag page, breadcrumb, or page header.

### Phase 2: reader shell

1. Make the main grid's available width explicit: files panel is outside the reader grid; the reader grid contains article, resizer, and optional rail.
2. Center the article-plus-rail group in `.mk-main`, with a small intentional optical shift only if browser screenshots confirm it improves balance.
3. Keep the article readable at the default width and preserve thin/wide/full modes without duplicating conflicting grid rules.
4. Remove stale CSS overrides so each layout mode has one authoritative rule.

Acceptance: with the files panel open, the Markdown column is centered in the space to its right, not centered against the full viewport and not pushed under the files panel.

### Phase 3: Quartz-style navigation hierarchy

1. Treat the files panel as the primary location for browsing folders and pages.
2. Keep breadcrumbs as filesystem context above the article.
3. Make the TOC heading-aware and hide it for short documents.
4. Keep backlinks and unlinked mentions below the document or in the rail, never above the reading content.
5. Use the same display title in all navigation surfaces.

Acceptance: a user can identify location, document title, and document structure without duplicate labels or unexplained metadata blocks.

### Phase 4: typography and Markdown presentation

1. Establish one readable body measure and line height for prose.
2. Make heading hierarchy visually distinct without styling every H1 as a page title.
3. Preserve hard wraps in code and technical text while allowing prose to reflow normally.
4. Make tables, images, formulas, and code blocks scroll or resize within the article instead of expanding the page shell.
5. Keep metadata typography visibly quieter than document typography.

Acceptance: the supplied technical corpus remains readable without horizontal page overflow or collapsed cards.

### Phase 5: interaction and feedback

1. Keep the file tree read-only: no drag/drop, rename, move, delete, or Trash UI in v0.0.2.
2. Keep content editing and page creation explicit, with feedback tied to the saved Markdown file.
3. Keep lazy-loaded editor/icon resources scoped to actual feature activation, not ordinary document navigation.

Acceptance: navigation never mutates vault paths; editing changes only the selected Markdown file and reports save failures clearly.

### Phase 6: verification

Run the repository checks plus browser evidence:

- Markdown unit tests for title and heading behavior;
- Rust workspace tests and clippy;
- CSS generation/checks;
- real HTTP black-box checks for title/search/tag/breadcrumb consistency;
- Playwright checks at desktop, tablet, and mobile widths;
- screenshot review with the files panel open and closed;
- long-title, long-path, hard-wrap, code-block, table, and empty-TOC cases.

The final review must report exact routes, fixtures, viewport sizes, and which behaviors were verified in a real running Miku process.
