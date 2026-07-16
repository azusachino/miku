# CLAUDE.md

## Project Overview

Miku (初音ミク / ミク) is a filesystem-owned personal Markdown wiki: a browser editor over plain `.md` files with server-side, background indexing for backlinks, tags, and full-text search.

## Tech Stack & Architecture

- **Language:** Rust (edition 2021)
- **Web:** axum + tokio + tower-http (serves `static/` + page/API routes)
- **Index cache:** Postgres via sqlx (disposable; rebuildable from files)
- **Markdown:** comrak (native wikilinks + GitHub `[!NOTE]` alerts + GFM)
- **Filesystem watch:** notify (background indexer)
- **Frontend (v0):** server-rendered HTML + plain `<textarea>` (no JS bundler)

**Core invariant:** Markdown files + assets under `miku/` are the source of truth. Postgres holds only a disposable index (`pages`, `links`, `tags`, FTS) that is fully rebuildable from `miku/**/*.md`.
See `miku_docs/architecture.md`.

**Single-writer model:** HTTP handlers are read-only against Postgres; the background indexer is the sole writer. Saves are atomic (temp + rename) and the `notify` watcher is the _only_ index trigger
— no double-indexing, no races.

## Commands

```bash
make fmt          # Format all files (cargo fmt + prettier)
make lint         # cargo clippy -D warnings
make test         # cargo test
make check        # fmt-check + lint + test (run before commit)
make validate     # check + build (run before PR)
make run          # run the server
```

All daily operations go through `make <target>`. Tools come from the Nix devShell — enter with `nix develop`, or run one-off via `nix develop --command <cmd>` (the Makefile wraps commands
automatically). Project scripting/automation is Python run via `uv run` (root `pyproject.toml`), not bash.

## Coding Conventions

- Conventional commits: `feat:`, `fix:`, `chore:`, `deploy:` — no emojis
- 4-space indent for Rust; 2-space for config files (TOML/JSON/YAML)
- Errors via `anyhow`/`thiserror`; no manually wrapped prose in docs
- Stage files explicitly (`git add <files>`) — never `git add -A`/`.`

## Quality Standards

- `make check` must pass before commit; `make validate` before PR
- Never recompute the whole graph on a keystroke; index changed pages in the background; paginate/virtualize backlinks

## Rules

- See `.claude/rules/core.md` for agent DO/DON'T rules
- See `.claude/rules/config.md` for config / migration rules
- See `.claude/rules/testing.md` for testing conventions (path-scoped)

## Key Files

- `src/main.rs` — binary entry point (axum server)
- `src/lib.rs` — crate root
- `migrations/` — sqlx Postgres migrations (index schema)
- `miku_docs/architecture.md` — design, schema, save/index contract
- `static/` — server-rendered assets
