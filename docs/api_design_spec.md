# Miku HTTP API Design Specification

This document details a robust, structured API design for the Miku personal wiki. It separates standard HTML multi-page application (MPA) routes from JSON REST endpoints to support interactive UI features (like the `Ctrl-K` palette and `[[wikilink]]` autocomplete).

---

## 1. Routing Taxonomy & Scope

We divide the routing table into three distinct namespaces:
1. **`/p/*path` (HTML Pages)**: Server-rendered wiki pages (view, edit, list).
2. **`/api/*` (JSON REST API)**: Asynchronous endpoints for frontend interactions.
3. **`/static/*` (Assets)**: Static CSS, JS templates, and user-uploaded media from `miku_docs/assets/`.

---

## 2. Server-Rendered HTML Routes (MPA)

These routes handle the core Multi-Page Application (MPA) flow, working with Javascript disabled.

| Method | Route | Description | Query Parameters / Forms |
| :--- | :--- | :--- | :--- |
| **GET** | `/` | Redirects to home page note (configured title, default `Index`). | None |
| **GET** | `/p/*path` | View a rendered Markdown page (from `miku_docs/`). Support nested subfolders (e.g. `/p/work/project-a`). | None |
| **GET** | `/p/*path/edit` | Render the markdown editor page (textarea for `miku_docs/` pages). | None |
| **POST** | `/p/*path` | Save a modified page (atomic write + rename into `miku_docs/`). | Form: `body` (Markdown), `loaded_hash` (optimistic concurrency checking). |
| **GET** | `/search` | Render the full-text search results page (searches Miku index). | `q` (search query) |
| **GET** | `/tags` | View tag cloud or hierarchical tag index. | None |
| **GET** | `/tags/*tag` | View list of pages containing `#tag` or nested `#tag/subtag`. | None |
| **GET** | `/backlinks/*path` | Dedicated backlinks and unlinked mentions page. | `page` (pagination offset) |

---

## 3. JSON REST APIs (`/api/v1/*`)

These endpoints support the command palette (`Ctrl-K`), autocomplete, and dynamic page management.

### A. Autocomplete & Navigation
#### 1. Fuzzy Autocomplete
- **Route:** `GET /api/v1/autocomplete`
- **Description:** Returns a quick, fuzzy-matched list of pages for `[[wiki link]]` autocompletion. Uses GIN `pg_trgm` indexes on slugs/titles/aliases.
- **Query Params:** `q=thermo`
- **Response (200 OK):**
  ```json
  [
    {
      "title": "Thermodynamics Lecture 2",
      "slug": "thermodynamics-lecture-2",
      "path": "physics/thermo-2.md",
      "match_type": "title"
    },
    {
      "title": "Entropy",
      "slug": "entropy",
      "path": "physics/entropy.md",
      "match_type": "alias"
    }
  ]
  ```

#### 2. Page Directory Listing
- **Route:** `GET /api/v1/pages`
- **Description:** Returns all indexed pages (from `miku_docs/`), titles, and paths. Used by frontend to build a local cache for instant search/navigation.
- **Response (200 OK):**
  ```json
  [
    {
      "path": "Index.md",
      "slug": "index",
      "title": "Home Index",
      "tags": ["index", "meta"],
      "mtime": 1719323145
    }
  ]
  ```

### B. Page Actions
#### 1. Safe Rename / Refactor
- **Route:** `POST /api/v1/page/rename`
- **Description:** Renames a file and rewrites all incoming links.
- **Payload:**
  ```json
  {
    "old_path": "work/project-a.md",
    "new_path": "projects/miku.md",
    "dry_run": false
  }
  ```
- **Response (200 OK - Dry Run):**
  ```json
  {
    "dry_run": true,
    "backlinks_to_rewrite": 8,
    "referencing_files": ["Index.md", "work/todo.md"]
  }
  ```
- **Response (200 OK - Execution completed):**
  ```json
  {
    "dry_run": false,
    "files_updated": 9,
    "message": "Renamed work/project-a.md to projects/miku.md. Updated 8 references."
  }
  ```

#### 2. Soft Delete
- **Route:** `DELETE /api/v1/page/*path`
- **Description:** Soft-deletes a page (moves from `miku_docs/` to `miku_docs/.trash/`) with a timestamp suffix.
- **Response (200 OK):**
  ```json
  {
    "path": "sub/Bar.md",
    "deleted_at": 1719323145,
    "trash_path": ".trash/sub/Bar@1719323145.md",
    "backlinks_dangling": 3
  }
  ```

### C. Trash Management
#### 1. List Trash
- **Route:** `GET /api/v1/trash`
- **Response (200 OK):**
  ```json
  [
    {
      "original_path": "sub/Bar.md",
      "trash_path": ".trash/sub/Bar@1719323145.md",
      "deleted_at": 1719323145
    }
  ]
  ```

#### 2. Restore Page
- **Route:** `POST /api/v1/trash/restore`
- **Payload:**
  ```json
  {
    "trash_path": ".trash/sub/Bar@1719323145.md"
  }
  ```
- **Response (200 OK):**
  ```json
  {
    "restored_path": "sub/Bar.md",
    "message": "Page sub/Bar.md successfully restored. Backlinks auto-resolved."
  }
  ```

### D. Media Assets
#### 1. Hash-Deduped Upload
- **Route:** `POST /api/v1/assets`
- **Description:** Uploads a file (multipart form) into `miku_docs/assets/`, renaming it to `filename-<short-hash>.ext` if there's a name collision.
- **Response (201 Created):**
  ```json
  {
    "filename": "chart-a1b2c3d.png",
    "url": "/static/assets/chart-a1b2c3d.png",
    "embed_syntax": "![[chart-a1b2c3d.png]]"
  }
  ```

---

## 4. Error Handling & HTTP Status Codes

We define a strict error-contract mapping domain errors to appropriate HTTP status codes:

| Scenario | HTTP Code | JSON Error Payload |
| :--- | :--- | :--- |
| **Optimistic Lock Fail** | `409 Conflict` | `{"code": "EDIT_CONFLICT", "message": "File modified on disk since loaded. Please merge changes."}` |
| **Slug Collision on Create** | `422 Unprocessable` | `{"code": "SLUG_COLLISION", "message": "A page with this name already exists in a different folder."}` |
| **Page Not Found** | `404 Not Found` | `{"code": "PAGE_NOT_FOUND", "message": "Page sub/Bar.md does not exist."}` |
| **Invalid Page Name** | `400 Bad Request` | `{"code": "INVALID_NAME", "message": "Page names cannot contain reserved characters ([], #, ?, *)."}` |
| **Rate Limit / DB Overload** | `429 Too Many Requests` | `{"code": "RATE_LIMIT_EXCEEDED", "message": "Write operations throttled. Please try again in a moment."}` |
