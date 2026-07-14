# Plan

Tracked durably in the asobi graph under epic `miku:mvp`. This file mirrors it
for humans.

## Current phase

v0 — vertical slice (skeleton → CRUD edit/save/render loop).

## MVP scope (locked 2026-06-25)

In:

1. Markdown page CRUD (content root `miku_docs/`)
2. `[[wiki links]]` parse / resolve / render
3. Backlinks
4. Tags (`#tag`)
5. Full-text search (Postgres `tsvector`)
6. Atomic async saves
7. Background indexer (`notify` → parse → Postgres)

Roadmap (post-MVP): drag/drop image upload to `assets/`, CodeMirror 6 editor.

Dropped: daily note, Obsidian import (Obsidian `[[ ]]` compatibility is covered
by wiki-links).

## Milestones

- **v0:** skeleton + trimmed deps; read/render a page; edit + atomic save.
- **v1:** background indexer; `[[links]]`; backlinks; tags; full-text search.
- **v2:** image upload; CodeMirror 6 editor.

## Decisions

- Postgres index — see `docs/architecture.md` and asobi
  `miku:decision:postgres-index`.
- v0 frontend = server-rendered + textarea — asobi
  `miku:decision:frontend-v0-textarea`.
