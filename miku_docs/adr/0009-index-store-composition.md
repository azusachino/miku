---
id: ADR-0009
type: adr
title: ADR-0009 — Index store and cache composition
slug: index-store-composition
status: Accepted
updated: 2026-07-14
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:index-store-composition
supersedes: []
superseded-by:
relates-to: [ADR-0006]
impacts: [src/main.rs, src/indexer.rs, crates/miku-domain]
config-keys: [MIKU_TIER, MIKU_PRIMARY, MIKU_CACHE, MIKU_EVENTS]
tags: [backend, index, cache, architecture]
---

# ADR-0009 — Index store and cache composition

## Decision

Markdown files under `miku_docs/` remain the only source of truth. The database is a rebuildable index projection.

Miku composes one durable `IndexStore` with zero or more read/cache layers:

- `MemoryCache` is the process-local L1 cache in every real deployment;
- SQLite or Postgres is the durable primary `IndexStore`;
- Valkey is an optional L2 cache and event transport for the scale profile;
- an in-process `EventBus` remains the default local event transport.

The standalone memory implementation exists for tests and disposable runs. It is not a silent production fallback.

## Why

This preserves the files-owned invariant while allowing a small local profile and a larger multi-process profile. Naming the durable boundary `IndexStore` avoids conflating primary storage, cache, and
event transport under “backend”.

## Trade-offs / Rejected

- Rejected Valkey as authoritative storage: cache loss must not affect correctness or Markdown durability.
- Rejected automatic fallback between durable primaries: a misconfigured or unavailable primary must be visible rather than silently changing semantics.
- Deferred distributed-only SSE: in-process events are sufficient until multiple Miku processes are an actual deployment requirement.
