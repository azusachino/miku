---
id: ADR-0014
title: Native Turso 0.7 local backend
slug: native-turso-07-local-backend
status: Superseded
date-proposed: 2026-07-14
date-accepted: 2026-07-14
deciders: [haru]
mirror: asobi:miku:decision:native-turso-07-local-backend
supersedes: [ADR-0011]
superseded-by: [ADR-0016]
relates-to: [ADR-0009, ADR-0010]
impacts: [crates/miku-index-turso, crates/miku-app, Makefile, docs/setup.md]
config-keys: [MIKU_INDEX_BACKEND, MIKU_INDEX_PATH]
tags: [backend, turso, local, rust]
---

# ADR-0014 — Native Turso 0.7 local backend

## Decision

The default local durable index is the Rust-built Turso Database engine through the `turso = "0.7.0"` crate. The implementation lives in `miku-index-turso` and uses Turso's native async connection,
transaction, and FTS index APIs.

The local runtime selects it with `MIKU_INDEX_BACKEND=turso` and stores the database at `miku_docs/.miku-index.turso` by default. SQLite3, libSQL, and SQLx-SQLite are not local backend dependencies or
compatibility targets for this adapter.

## Why

This matches the chosen engine and keeps the backend boundary honest: Miku's domain contract is independent of the storage implementation, while the local profile exercises the actual Turso Rust
engine and its native FTS path.

## Trade-offs / Rejected

- Rejected SQLx-SQLite as a substitute: it would silently select a different database engine than the project decision.
- Rejected libSQL as an alias for Turso Database: they are separate projects with separate Rust APIs.
- Accepted Turso's experimental FTS index feature because full-text search is a required local capability; its behavior is covered by backend tests and real corpus black-box checks.
