# miku

Miku (ミク) — a filesystem-owned personal Markdown wiki with a browser editor and server-side background indexing (backlinks, tags, full-text search).

Markdown files under `miku_docs/` are the source of truth; Postgres holds only a disposable, rebuildable index.

## Dependencies

- Postgres

## Docs

- `docs/architecture.md` — design, schema, save/index contract
- `docs/setup.md` — build & run
- `docs/plan.md` — roadmap
