# CLAUDE.md

## Project overview

Miku Note is a filesystem-owned Markdown workspace. Markdown files and assets
under miku_docs/ are the source of truth; indexes are disposable projections.

## Technology

- Rust 2021 workspace with axum, tokio, notify, and SQLx.
- miku-domain: backend-neutral records and projection contracts.
- miku-vault: safe, atomic Markdown filesystem operations.
- miku-app: application services and backend composition.
- miku-web: React, TypeScript, Vite, Tailwind, and TanStack Query.
- Markdown reader: React Markdown, GFM alerts, Prism, Mermaid, and KaTeX.

## Commands

~~~bash
nix develop
make dev
make check
make check-all-features
make check-blackbox
make check-ux-browser
make validate
~~~

All daily operations go through the Makefile. Python automation runs through
uv and Rust/frontend tools come from the Nix devShell.

## Architecture rules

- Keep Markdown source independent from indexes and caches.
- Keep HTTP handlers read-oriented; filesystem changes flow through the vault
  and watcher.
- Keep frontend code grouped by app, feature, component, and shared concerns.
- Use Obsidian-style YAML frontmatter for first-party notes.
- Add or update an ADR when changing a durable architectural boundary.

## Quality and commits

Run make check before committing and make validate before opening a PR. Use
conventional commits and stage files explicitly. Never commit local indexes,
runtime artifacts, screenshots, or secrets.

See miku_docs/architecture.md, miku_docs/Features.md, and miku_docs/adr/ for
the current product and architecture contracts.
