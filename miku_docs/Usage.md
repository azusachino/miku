---
title: Using Miku Note
type: guide
status: active
tags: [miku, guide, setup]
updated: 2026-07-16
---

# Running Miku Note

This page covers local setup, the content directory, and the commands used to
run Miku Note. #guide

## Prerequisites

- Nix with flakes, which provides the pinned Rust and development tools;
- Postgres only when using the explicit Postgres profile or the container stack;
- Podman or Docker only when using the optional compose stack.

## Quick start

For the default local profile:

```bash
make dev
```

The server starts at `http://localhost:3000` and uses the Rust-built SQLite
index at `miku_docs/.miku-index.sqlite`. The index is disposable; the Markdown
files remain the source of truth.

To verify the optional Postgres/Valkey stack:

```bash
make compose-experiments
```

The compose service uses Postgres and exposes Miku Note on port `3000`.

## Content directory

Pages live under `miku_docs/` and are plain Markdown files. For example:

```text
miku_docs/Features.md
miku_docs/guides/Getting Started.md
```

They are available at `/p/Features.md` and `/p/guides/Getting%20Started.md`.
Wikilink matching is case-insensitive and supports aliases:
`[[Features|What it does]]`.

Miku does not create a Trash directory. Assets belong in
`miku_docs/assets/`; path changes and file removal remain ordinary filesystem
operations outside the v0.0.2 UI.

## Writing Markdown

Miku Note uses Comrak with GFM-style tables, task lists, strikethrough,
autolinks, alerts, wikilinks, and raw HTML for trusted local files. The reader
also supports:

- `#tags` and YAML frontmatter properties;
- fenced code blocks, Mermaid diagrams, and `$...$` / `$$...$$` math;
- `![[asset.png]]` embeds.

See [[Sandbox]] for examples and [[Features]] for the complete current list.

## Editing and external changes

Open a page at `/p/...` and choose **Edit** for the inline CodeMirror editor.
Saves are atomic and guarded by a content hash so an edit made elsewhere is not
silently overwritten.

The filesystem watcher notices changes made by git, an editor, or scripts and
updates the index in the background. Reader mode checks for a newer indexed
version without holding an idle event stream open.

## Rebuilding the index

The index can always be rebuilt from the files:

```bash
rm -f miku_docs/.miku-index.sqlite
make dev
```

For a Postgres deployment, drop or recreate the disposable database and start
the server with `MIKU_INDEX_BACKEND=postgres`; migrations run on startup.

## Checks and browser acceptance

```bash
make check
make check-ux-browser
```

The browser acceptance command requires a local Playwright browser installation
and verifies the real reader, lazy assets, navigation, tags, editor, and narrow
layout behavior.
