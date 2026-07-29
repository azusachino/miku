---
id: ADR-0022
type: adr
title: ADR-0022 — Backend-Driven Note Context & Outgoing Link Resolution
slug: backend-note-context-outgoing-links
status: Accepted
updated: 2026-07-29
date-proposed: 2026-07-29
date-accepted: 2026-07-29
deciders: [haru]
mirror: asobi:miku:decision:backend-note-context-outgoing-links
supersedes: []
superseded-by: []
relates-to: [ADR-0017, ADR-0019, ADR-0020]
impacts: [crates/miku-app, crates/miku/src/http_api.rs, miku-web]
tags: [note-context, outgoing-links, api, performance]
---

# ADR-0022 — Backend-Driven Note Context & Outgoing Link Resolution

## Decision

Expand `ContextResponse` DTO (`GET /api/v1/note-context/{id}`) to include a resolved `outgoing` links array alongside `note`, `parents`, `children`, and `backlinks`.

The backend `note_context()` service resolves every outgoing link in the requested note against the index at query time:
* **Existing Target**: Resolves target stem/alias to canonical vault path (e.g. `reference-note` $\rightarrow$ `vault/maps/reference-note.md`, `is_missing: false`).
* **Uncreated Target**: Resolves relative path within current note folder directory (e.g. `uncreated-note` $\rightarrow$ `vault/maps/uncreated-note.md`, `is_missing: true`).

The frontend React client renders `context.data.outgoing` directly from the REST payload without parsing Markdown body regexes or prefetching global vault pages.

## Why

* **Eliminate 7s Latency at Scale**: Global vault page dumps (`/api/v1/pages`) required loading 15,911 SQLite rows and transmitting multi-megabyte JSON payloads over HTTP on app load, causing 7-second UI stalls.
* **Single Lightweight Fetch**: `GET /api/v1/note-context/{id}` executes in **<15ms** and delivers complete context for rendering note body, backlinks, and outgoing links.
* **Authoritative Graph Resolution**: The backend indexer owns title folding, alias resolution, and target path computation, preventing client-side path ambiguity.

## Trade-offs / Rejected

* **Rejected**: Client-side global page dumps (`/api/v1/pages`). Prefetching 15,911 notes on startup fails scale constraints.
* **Rejected**: Client-side regex parsing of outgoing links. Client-side parsing lacks access to unexpanded folder notes and fails when resolving cross-directory links.
