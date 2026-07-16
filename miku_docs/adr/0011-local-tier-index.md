---
id: ADR-0011
title: Local deployment tier
slug: local-tier-index
status: Superseded by ADR-0016
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:local-tier-index
supersedes: []
superseded-by: [ADR-0016]
relates-to: [ADR-0009, ADR-0010]
impacts: [crates/miku-index-sqlite, Makefile, miku_docs/setup.md]
config-keys: [MIKU_INDEX_BACKEND, MIKU_INDEX_PATH]
tags: [backend, sqlite, local]
---

# ADR-0011 — Local deployment tier

This decision is superseded by ADR-0016. The current local durable index is SQLite via SQLx, selected by `MIKU_INDEX_BACKEND=sqlite` and stored at `miku_docs/.miku-index.sqlite` by default.

## Historical scope

The local profile provides a durable, portable, low-operations index while Markdown remains independently rebuildable. Memory remains an explicit test/disposable mode, and Postgres remains the
optional scale-profile dependency.
