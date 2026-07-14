# Running Miku

This page explains how to set up and run the Miku wiki server locally, and how to manage your content directory. #guide

## Prerequisites

Miku is built in Rust and uses Postgres for indexing. You'll need:

- Rust 1.70+ (or use the Nix devShell)
- Postgres 12+
- Docker / Podman (optional, for the compose stack)

## Quick Start

### Using Docker Compose

The easiest way to run Miku is with the provided `compose.yml`:

```bash
podman compose up
```

This starts:
- A Postgres container for the index
- The Miku server on `localhost:3000`

Your `miku/` content directory is automatically volume-mounted, so edits are immediately visible.

### Manual Setup

If you prefer to manage Postgres separately:

1. Create a Postgres database:
   ```sql
   CREATE DATABASE miku_index;
   ```

2. Set the connection string:
   ```bash
   export DATABASE_URL="postgres://user:password@localhost/miku_index"
   ```

3. Run migrations:
   ```bash
   make validate
   ```

4. Start the server:
   ```bash
   make run
   ```

The wiki is now live at `http://localhost:3000`.

## Content Directory

All your wiki pages live in the `miku/` directory at the repo root. Files are plain `.md` (Markdown), using GitHub Flavored Markdown plus [[wikilinks]].

### Creating Pages

Create a new `.md` file in `miku/`. The filename (minus the extension) becomes the page name. For example, `miku/Features.md` is accessible as [[Features]].

Page names are case-sensitive in the URL but case-insensitive for wikilink resolution, so `[[features]]` and `[[Features]]` both link correctly.

### Deleting Pages

Delete the `.md` file from `miku/`. The page is removed from the index automatically within a few seconds as the background [[Features|indexer]] picks up the change.

### Organizing with Folders

You can organize pages into subfolders: `miku/guides/Getting_Started.md` becomes [[guides/Getting_Started]]. The same atomic-save and background-index guarantees apply.

## Writing Markdown

See [[Sandbox]] for syntax examples and wikilink demonstrations. #docs

Miku uses [comrak](https://github.com/kivikakk/comrak), which supports:
- GitHub Flavored Markdown (tables, strikethrough, autolinks)
- Miku wikilinks: `[[PageName]]` or `[[PageName|Link Text]]`
- GitHub alerts: `> [!NOTE] This is a note`

## Rebuilding the Index

If you manually edit files outside the browser, or suspect the index is stale, trigger a full rebuild:

```bash
# SQL-based rebuild (clears and repopulates from disk)
make rebuild-index
```

Or restart the server — it rebuilds the index on startup if the database is empty.

## Architecture & Implementation

For details on how the background indexer works, how atomic saves guarantee consistency, and how wikilink resolution is implemented, see the architecture documentation. The key invariant is: **files in `miku/` are the source of truth; Postgres holds only a rebuildable index**. See [[Index]] for more context. #guide

---

Questions? See [[Features]] for what Miku can do, or browse the wiki starting from [[Index]].
