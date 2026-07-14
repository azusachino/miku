---
id: ADR-0010
title: Cargo workspace and crate boundaries
slug: workspace-crate-boundaries
status: Accepted
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:workspace-crate-boundaries
supersedes: []
superseded-by:
relates-to: [ADR-0007, ADR-0009]
impacts: [Cargo.toml, src, crates]
config-keys: []
tags: [rust, cargo, tokio, architecture]
---

# ADR-0010 — Cargo workspace and crate boundaries

## Decision

Miku becomes a Cargo workspace with focused packages:

- `miku-domain` — domain records, `IndexStore` contract, capabilities, errors;
- `miku-markdown` — parsing, rendering, and index projection extraction;
- `miku-indexer` — filesystem watcher, reconcile loop, and index events;
- `miku-index-memory` — reference/test implementation;
- `miku-index-turso` — local SQLite/Turso implementation;
- `miku-index-postgres` — Postgres implementation;
- `miku-cache-valkey` — optional Valkey cache/event integration;
- `miku-app` — Axum binary, templates, startup, and composition root.

The dependency direction is toward `miku-domain`. Backend crates never depend
on the HTTP application crate. Library crates do not create a Tokio runtime;
only `miku-app` owns `#[tokio::main]`.

Long-lived indexer tasks use explicit cancellation and shutdown ownership. Public
library errors use `thiserror`; application orchestration may use `anyhow`.

## Why

Separate packages keep optional database drivers out of unrelated builds and
make backend contract tests executable across implementations. The workspace
also provides a natural boundary for learning idiomatic Tokio ownership,
tracing, bounded channels, and graceful shutdown.

## Trade-offs / Rejected

- Rejected one crate per module: it adds navigation cost without an API boundary.
- Rejected a separate HTTP API crate for now: route DTOs are application-owned,
  while the reusable contract is the `IndexStore` API.
- Deferred splitting `miku-markdown` if the first extraction shows it is too
  small to justify an independent package.
