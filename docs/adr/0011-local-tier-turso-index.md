---
id: ADR-0011
title: Local deployment tier with Turso index
slug: local-tier-turso-index
status: Superseded
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:local-tier-turso-index
supersedes: []
superseded-by: [ADR-0016]
relates-to: [ADR-0009, ADR-0010]
impacts: [crates/miku-index-turso, Makefile, docs/setup.md]
config-keys: [MIKU_TIER, MIKU_PRIMARY, MIKU_DB_PATH, TURSO_DATABASE_URL, TURSO_AUTH_TOKEN]
tags: [backend, sqlite, turso, local]
---

# ADR-0011 — Local deployment tier with Turso index

## Decision

The default deployment profile is `local`:

```text
MemoryCache + Turso/SQLite IndexStore + in-process EventBus
```

The durable primary is local SQLite/Turso-compatible storage. Remote Turso connectivity is optional and must not be required for local startup. A pure memory store is explicit test/disposable mode
only.

The Rust driver and local/remote configuration are a foundation spike. The spike must verify migrations, FTS behavior, backup/rebuild, and bounded handling of SQLite/Turso write contention before the
driver is treated as stable.

## Why

This gives a personal deployment a durable, portable, low-operations index while keeping the Markdown tree portable and independently rebuildable. The process local cache makes common reads cheap
without introducing a required service.

## Trade-offs / Rejected

- Rejected memory as the default durable store: restart would discard the index.
- Rejected Postgres as the default personal dependency: it imposes unnecessary service setup for the local profile.
- Deferred remote Turso as a required availability dependency until sync and auth behavior have dedicated verification.
