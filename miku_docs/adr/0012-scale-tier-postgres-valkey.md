---
id: ADR-0012
title: Scale deployment tier with Postgres and Valkey
slug: scale-tier-postgres-valkey
status: Accepted
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:scale-tier-postgres-valkey
supersedes: []
superseded-by:
relates-to: [ADR-0009, ADR-0010]
impacts: [crates/miku-index-postgres, crates/miku-cache-valkey, compose.yml]
config-keys: [MIKU_TIER, MIKU_PRIMARY, VALKEY_URL, DATABASE_URL]
tags: [backend, postgres, valkey, scale]
---

# ADR-0012 — Scale deployment tier with Postgres and Valkey

## Decision

The high-end deployment profile is `scale`:

```text
MemoryCache + optional Valkey L2/cache-events + Postgres IndexStore
```

Postgres is the durable primary and retains its native FTS/trigram capabilities behind the backend-neutral contract. Valkey accelerates repeated reads and may carry invalidation/pub-sub events, but it
is never authoritative.

A Valkey outage degrades to the in-process cache and Postgres. There is no silent fallback from Postgres to SQLite or memory.

## Why

This keeps the high-end profile operationally strong without forcing Valkey and Postgres onto local users. The same application contract and fixture suite can prove semantic parity while allowing
Postgres-specific ranking quality.

## Trade-offs / Rejected

- Rejected Valkey-primary indexing: cache loss must be safe.
- Rejected distributed pub/sub as a local requirement: one process needs only an in-process event bus.
- Accepted different ranking scores across stores; result semantics must match, but backend-specific ranking is allowed.
