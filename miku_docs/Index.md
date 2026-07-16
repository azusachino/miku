---
title: Miku Note
type: index
status: active
tags: [miku, guide]
updated: 2026-07-16
---

# Miku Note: Notes You Still Own

Miku Note is a filesystem-owned personal Markdown wiki. Your notes stay as
ordinary files under `miku_docs/`; the browser is a reader and editor, while a
rebuildable background index supplies links, tags, mentions, and search. #docs
#feature

## What Miku Note does

- **Keeps files portable** — edit the same `.md` files in Miku Note, git, or a
  text editor.
- **Connects notes** — `[[wikilinks]]`, aliases, backlinks, and unlinked-mention
  discovery turn a folder into a navigable knowledge network.
- **Finds content** — embedded ripgrep searches Markdown source, while the
  disposable index keeps page switching, tags, and relationships fast.
- **Reads quickly** — direct `/p/...` URLs open inside the persistent browser
  shell, and switching pages keeps shared assets mounted.
- **Renders Markdown well** — alerts, tables, code highlighting, Mermaid, and
  math are available without making every reader page pay their asset cost.

## Start here

- [[Features]] — the current feature inventory and scope boundary.
- [[Usage]] — local setup, content paths, and the development commands.
- [[Sandbox]] — live examples of Markdown, diagrams, code, math, and links.
- [[Changelog]] — shipped and in-progress product changes. #release

## Project knowledge

- [[architecture]] — system boundaries, storage, indexing, and link resolution.
- [[dataflow]] — Mermaid workflows for indexing, reading, and saving notes.
- [[setup]] — local development, backends, and verification commands.
- [[runtime-workflow]] — how filesystem changes become reader state.
- [[product]] — product direction and UX constraints.
- [[adr/README]] — verified ADR index; individual records live below `adr/`.
- [[api]] — current JSON and browser route contract.

## Core invariant

`miku_docs/**/*.md` is the source of truth. The configured local index backend
is disposable and rebuildable; editing or deleting the index never deletes your
notes.

## Release history

The original MVP shipped as v0.0.1. The current working line is the Miku Note
frontend and reader refresh; see [[Changelog]] for the details and status.
