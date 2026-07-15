---
id: ADR-0004
title: Rename / delete & assets
slug: rename-delete-assets
status: Superseded
date-proposed: 2026-06-25
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:rename-delete-assets
supersedes: []
superseded-by:
relates-to: [ADR-0003]
depends-on: [ADR-0002]
impacts: []
config-keys: []
tags: [superseded, historical, assets]
---

# ADR-0004 — Rename / delete & assets

> Superseded for Miku v0.0.2. This design is retained as historical context but is not implemented or shipped. v0.0.2 keeps the file tree read-only and supports Markdown content editing only; users
> manage paths and deletion in their filesystem.

## Current decision

This proposal is superseded and not part of v0.0.2. The explorer is read-only; Miku exposes no move, rename, delete, Trash, restore, or purge UI/API. Content editing remains available through the
Markdown editor. Users manage paths and file removal with the filesystem, editor, scripts, or git, and the watcher reconciles those changes.

Asset handling remains separate: assets live in `miku_docs/assets/` and are never automatically deleted by Miku.
