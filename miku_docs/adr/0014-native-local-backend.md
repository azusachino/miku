---
id: ADR-0014
title: Native local backend
slug: native-local-backend
status: Superseded by ADR-0016
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:native-local-backend
supersedes: [ADR-0011]
superseded-by: [ADR-0016]
relates-to: [ADR-0009, ADR-0010]
impacts: [crates/miku-index-sqlite, crates/miku-app, Makefile, miku_docs/setup.md]
config-keys: [MIKU_INDEX_BACKEND, MIKU_INDEX_PATH]
tags: [backend, sqlite, local, rust]
---

# ADR-0014 — Native local backend

This decision is superseded by ADR-0016. It is retained as the numbered decision record; the current implementation and configuration are defined by ADR-0016.

## Current boundary

The local runtime selects SQLite with `MIKU_INDEX_BACKEND=sqlite` and stores the database at `miku_docs/.miku-index.sqlite` by default. The backend implements the domain contract through SQLx and
SQLite FTS5.
