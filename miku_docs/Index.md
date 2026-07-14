# Miku: A Filesystem-Owned Personal Wiki

Miku (麒麟草) is a lightweight, filesystem-centric personal Markdown wiki with browser-based editing and server-side intelligence. Unlike wiki systems that treat files as secondary exports, Miku treats plain `.md` files in your `miku/` directory as the source of truth — they live alongside your site, fully version-controlled, and never locked behind a database.

## What Is Miku? #docs #feature

Miku is built around a core belief: **your notes should be plain text, owned by you, and searchable**. It provides:

- **Browser Editor**: Edit Markdown files in your browser; saves are atomic and trigger background indexing.
- **Wikilinks**: Link pages using `[[PageName]]` syntax — automatic backlink discovery means your wiki learns connections as you write.
- **Smart Indexing**: Full-text search, tag discovery, and backlink graphs computed in the background without slowing down reads.
- **No Lock-In**: Your content is just `.md` files — edit with any text editor, version-control with git, host anywhere.

## Core Features

See [[Features]] for a deep dive into wikilinks, backlinks, tags, full-text search, atomic saves, and the background indexer that powers Miku.

## Getting Started

To run Miku locally, see [[Usage]]. The app serves both the wiki interface and a REST API for index queries.

## Release History

Miku v0.0.1 shipped on 2026-06-26 as an MVP. See [[Changelog]] for details on what's included and planned for future releases. #release

## Playground

New to Miku? Check out [[Sandbox]] for a guided tour of Markdown syntax and wikilink behavior. #demo

---

Last updated: 2026-06-30
