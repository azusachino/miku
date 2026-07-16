<div align="center">

# Miku Note

**A calm, filesystem-owned Markdown wiki for people who want their notes to stay theirs.**

[![CI](https://github.com/azusachino/miku/actions/workflows/ci.yml/badge.svg)](https://github.com/azusachino/miku/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/built_with-Rust-orange.svg)](https://www.rust-lang.org/)

Plain Markdown files. Fast page navigation. Backlinks, tags, and search without giving up the filesystem.

[Features](#features) · [Quick start](#quick-start) · [Configuration](#configuration) · [Development](#development) · [Contributing](CONTRIBUTING.md)

</div>

## Why Miku Note?

Miku Note is a personal wiki with a deliberately small persistence model:

- Your notes and assets live as ordinary files under `miku_docs/`.
- The index is derived state. Delete it and Miku can rebuild it from Markdown.
- Reading is the primary experience: switching pages keeps the shell, styles, and JavaScript in place.
- Editing is explicit, with atomic saves and a background filesystem watcher.
- The server is written in Rust and the frontend is a separate React/Vite project.

This makes the vault easy to inspect, back up, version, or edit with another tool. Miku does not require a hosted account or a proprietary data format.

## Features

| Area            | What is included                                                                                           |
| --------------- | ---------------------------------------------------------------------------------------------------------- |
| Markdown        | CommonMark rendering, fenced code blocks, math, Mermaid, callouts, and wikilinks                           |
| Navigation      | Folder tree, breadcrumbs, page quick-switch, hash links, and a page-local table of contents                |
| Knowledge graph | Backlinks, linked mentions, tags, and paginated tag views                                                  |
| Search          | Metadata quick-switch plus embedded full-text content search powered by Rust's grep/ignore crates          |
| Editing         | Browser editor, inline reader editing, preview, atomic writes, and conflict-aware saves                    |
| Runtime         | MemoryIndex/Tantivy hot projection by default; optional SQLite/Postgres durability and Valkey shared cache |
| UX              | Light/dark themes, reading-width modes, lazy editor/highlighter loading, and a focused command palette     |

## Quick start

The default development path uses SQLite as the durable projection and MemoryIndex/Tantivy as the hot projection. PostgreSQL and Valkey are optional infrastructure layers; Valkey is a shared cache,
not a replacement for Tantivy.

```bash
git clone https://github.com/azusachino/miku.git
cd miku
nix develop           # enter the devShell (rust, bun, uv, postgres, …)
make dev              # run Rust backend and Vite frontend together
```

The native local stack is started with `uv run python scripts/dev.py`, which launches Rust and Vite together. `bun` comes from the Nix devShell, so no separate Node/Bun install is needed.

Open <http://127.0.0.1:3000>. The default content root is `miku_docs/`; put a Markdown file there and refresh the page after the watcher indexes it.

See [`miku_docs/setup.md`](miku_docs/setup.md) for external Postgres, Tailscale/LAN access, containers, browser checks, and environment overrides.

## Configuration

The useful local switches are:

| Variable             | Default                        | Purpose                                                       |
| -------------------- | ------------------------------ | ------------------------------------------------------------- |
| `MIKU_INDEX_BACKEND` | `sqlite`                       | Select the durable SQLite/Postgres or explicit memory backend |
| `MIKU_INDEX_PATH`    | `miku_docs/.miku-index.sqlite` | Location of the local derived index                           |
| `MIKU_BIND`          | `0.0.0.0:3000`                 | Address exposed by the HTTP server                            |
| `MIKU_READONLY`      | unset                          | Deploy the reader without write operations                    |
| `DATABASE_URL`       | unset                          | Postgres connection string when using the Postgres profile    |
| `VALKEY_URL`         | unset                          | Optional Valkey endpoint for the scale profile                |

The vault is intentionally single-user and login-less at this stage. If the server is reachable beyond a trusted machine, put it behind the network or identity boundary appropriate for your
deployment.

## Architecture

```text
Markdown files + assets
          │
          ├── reader/editor ──> React/Vite browser shell
          │
          └── filesystem watcher ──> derived index
                                      ├── links and backlinks
                                      ├── tags and metadata
                                      └── full-text search
```

The HTTP layer reads pages from the filesystem. The background indexer is the sole index writer. A save is written atomically, then the watcher reindexes only the changed page. The index is never the
source of truth.

The main browser routes are:

- `/p/{path}` — read a Markdown page; `/p/{path}.md` is accepted as an alias.
- `/p/{path}/edit` — edit a page.
- `/tags` and `/tags/{tag}` — browse tags with incremental loading.
- `/healthz`, `/readyz`, and `/metrics` — local/runtime probes.

More detail lives in [`miku_docs/architecture.md`](miku_docs/architecture.md), [`miku_docs/runtime-workflow.md`](miku_docs/runtime-workflow.md), and the decision records under
[`miku_docs/adr/`](miku_docs/adr/).

## Development

Use the repository Makefile as the stable interface:

```bash
make check                  # formatting, CSS, lint, Python checks, Rust tests
make check-all-features     # compile and test every Cargo feature combination
make check-blackbox         # live HTTP checks against a running server
make check-ux-browser       # Playwright browser acceptance checks
make release                # crates.io leaf package dry-runs
make validate               # check plus release build
```

The generated Tailwind stylesheet is committed alongside its input so a checkout can serve the frontend immediately; `make check` regenerates it and verifies the result.

Before opening a pull request, run `make check`. Keep user content, local indexes, `.pgdata/`, screenshots, and other runtime artifacts out of commits.

## Project status

Miku Note is an early, actively evolving project. The current milestone is `v0.0.3`; the user-facing changelog is [`miku_docs/Changelog.md`](miku_docs/Changelog.md). APIs, templates, and configuration
may change while the core filesystem-first invariant remains stable.

## Contributing

Bug reports, documentation improvements, tests, and focused patches are welcome. Please read [`CONTRIBUTING.md`](CONTRIBUTING.md) before starting and use the issue templates when they fit the report.

## License

Miku Note is free software licensed under the [GNU General Public License v3.0 or later](LICENSE).
