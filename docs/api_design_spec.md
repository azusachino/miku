# Miku HTTP API Design Specification

> **Current for v0.0.2.** This document describes the shipped API only. Miku does not expose move, rename, delete, Trash, restore, or purge endpoints; path changes and file removal are managed outside
> the application. Content editing remains available through `POST /p/*path`.

Operational endpoints are intentionally outside the application API version: `GET /healthz` is the liveness probe, `GET /readyz` is the readiness probe, and `GET /metrics` is the Prometheus endpoint.
Application JSON endpoints use the versioned `/api/v1/` prefix.

This document details a robust, structured API design for the Miku personal wiki. It separates standard HTML multi-page application (MPA) routes from JSON REST endpoints to support interactive UI
features (like the `Ctrl-K` palette and `[[wikilink]]` autocomplete).

---

## 1. Routing Taxonomy & Scope

We divide the routing table into three distinct namespaces:

1. **`/p/*path` (HTML Pages)**: Server-rendered wiki pages (view, edit, list).
2. **`/api/*` (JSON REST API)**: Asynchronous endpoints for frontend interactions.
3. **`/static/*` (Assets)**: Static CSS, JS templates, and user-uploaded media from `miku_docs/assets/`.

---

## 2. Server-Rendered HTML Routes (MPA)

These routes handle the core Multi-Page Application (MPA) flow, working with Javascript disabled.

| Method   | Route            | Description                                                                                              | Query Parameters / Forms                                                  |
| :------- | :--------------- | :------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------ |
| **GET**  | `/`              | Redirects to home page note (configured title, default `Index`).                                         | None                                                                      |
| **GET**  | `/p/*path`       | View a rendered Markdown page (from `miku_docs/`). Support nested subfolders (e.g. `/p/work/project-a`). | None                                                                      |
| **GET**  | `/p/*path/edit`  | Render the markdown editor page (textarea for `miku_docs/` pages).                                       | None                                                                      |
| **POST** | `/p/*path`       | Save a modified page (atomic write + rename into `miku_docs/`).                                          | Form: `body` (Markdown), `loaded_hash` (optimistic concurrency checking). |
| **GET**  | `/search`        | Render the full-text search results page (searches Miku index).                                          | `q` (search query)                                                        |
| **GET**  | `/tags`          | View tag cloud or hierarchical tag index.                                                                | None                                                                      |
| **GET**  | `/tags/*tag`     | View list of pages containing `#tag` or nested `#tag/subtag`.                                            | None                                                                      |
| **GET**  | `/folders/*path` | Browse a folder view derived from indexed pages.                                                         | None                                                                      |

---

## 3. JSON REST APIs (`/api/v1/*`)

These endpoints support navigation, tags, search, mention promotion, and the background index event stream. They do not mutate filesystem paths.

### A. Autocomplete & Navigation

| Method | Route                          | Purpose                                                  |
| :----- | :----------------------------- | :------------------------------------------------------- |
| GET    | `/api/v1/pages/*path`          | Return one indexed page projection.                      |
| GET    | `/api/v1/nav/children?dir=...` | Load one folder's navigation children.                   |
| GET    | `/api/v1/quickswitch?q=...`    | Search indexed page metadata for the command palette.    |
| GET    | `/api/v1/content-search?q=...` | Search Markdown source content.                          |
| GET    | `/api/v1/tags`                 | Return indexed tag counts.                               |
| GET    | `/api/v1/tags/*tag/pages`      | Return pages for one tag.                                |
| POST   | `/api/v1/promote-mention`      | Convert one selected plain-text mention into a wikilink. |

### B. Content and page APIs

The only page mutation in v0.0.2 is content saving:

- **Route:** `POST /p/*path`
- **Description:** Atomically saves Markdown content after optimistic concurrency validation. The filesystem watcher reindexes the changed file.
- **Form:** `body` and `loaded_hash`.

There is deliberately no application API for changing a page path or deleting files. Those operations remain ordinary filesystem or git operations.

---

## 4. Error Handling & HTTP Status Codes

We define a strict error-contract mapping domain errors to appropriate HTTP status codes:

| Scenario                     | HTTP Code               | JSON Error Payload                                                                                        |
| :--------------------------- | :---------------------- | :-------------------------------------------------------------------------------------------------------- |
| **Optimistic Lock Fail**     | `409 Conflict`          | `{"code": "EDIT_CONFLICT", "message": "File modified on disk since loaded. Please merge changes."}`       |
| **Slug Collision on Create** | `422 Unprocessable`     | `{"code": "SLUG_COLLISION", "message": "A page with this name already exists in a different folder."}`    |
| **Page Not Found**           | `404 Not Found`         | `{"code": "PAGE_NOT_FOUND", "message": "Page sub/Bar.md does not exist."}`                                |
| **Invalid Page Name**        | `400 Bad Request`       | `{"code": "INVALID_NAME", "message": "Page names cannot contain reserved characters ([], #, ?, *)."}`     |
| **Rate Limit / DB Overload** | `429 Too Many Requests` | `{"code": "RATE_LIMIT_EXCEEDED", "message": "Write operations throttled. Please try again in a moment."}` |
