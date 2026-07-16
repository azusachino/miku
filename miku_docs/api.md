---
title: Miku HTTP API
type: reference
status: active
tags: [miku, api, rust]
updated: 2026-07-16
---

# Miku HTTP API

The Rust binary serves the browser frontend and a small versioned JSON API.
The API is read-oriented: Markdown files remain the source of truth, while
indexes are rebuildable projections.

## Operational routes

| Method | Route | Purpose |
| --- | --- | --- |
| GET | / | Service metadata |
| GET | /healthz | Liveness |
| GET | /readyz | Readiness and index status |
| GET | /metrics | Prometheus metrics |
| GET | /events | Filesystem update event stream |
| GET | /api/openapi.json | Generated OpenAPI document |

## Workspace routes

All application JSON routes use the /api/v1 prefix.

| Method | Route | Purpose |
| --- | --- | --- |
| GET | /api/v1/workspace | Workspace capabilities and note count |
| GET | /api/v1/tree | Root tree; use prefix for folder children |
| GET | /api/v1/notes/{id} | Read one Markdown note |
| PUT | /api/v1/notes/{id} | Save one note with optimistic revision |
| GET | /api/v1/note-context/{id} | Note, metadata, backlinks, and context |
| GET | /api/v1/note-children/{id} | Child placements for a note |
| GET | /api/v1/search | Title, content, or combined search |
| GET | /api/v1/tags | Indexed tags and counts |
| GET | /api/v1/tags/{tag}/notes | Notes carrying one tag |

The {id} value is a URL-encoded Markdown-relative path, normally ending in
.md. The API never exposes a workspace-root label as a user-facing breadcrumb.

## Save contract

PUT /api/v1/notes/{id} accepts the Markdown body, title, and the revision token
returned by the previous read. A stale token returns 409 Conflict. The vault
writes the source atomically; the watcher then schedules projection updates.

Path creation, rename, move, and deletion are filesystem operations for now.
They are intentionally not disguised as JSON mutations.

## Error contract

| Status | Meaning |
| --- | --- |
| 400 | Invalid path, query, or request body |
| 404 | Markdown note or tag does not exist |
| 409 | Optimistic revision conflict |
| 500 | Source or projection failure |

The generated OpenAPI document and Rust route definitions are authoritative.
Update this note when a route is intentionally added or removed.
