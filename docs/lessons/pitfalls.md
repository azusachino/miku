# Miku Project Pitfalls & Lessons Learned

## 2026-07-29 — global-page-dump-for-link-resolution

- **Status**: resolved (ADR-0022)
- **Tried**: Fetching all 15,911 page summaries over `/api/v1/pages` during frontend app initialization to resolve client-side wikilinks.
- **Why it failed**: Loading 15,911 page summaries from SQLite and serializing a multi-megabyte JSON payload caused a 7-second blocking HTTP request on app startup.
- **Also tried and superseded**: Resolving relative wikilinks purely on the client using `currentPath` (e.g. prepending the current note's folder to an unresolved target). This avoided the page dump but still left path ambiguity, alias resolution, and folder-relative fallback logic duplicated between frontend and backend.
- **Do instead**: `GET /api/v1/note-context/{id}` resolves and returns `outgoing` links server-side (existing targets to their canonical path, uncreated targets to a folder-relative path), per ADR-0022. The frontend renders `context.data.outgoing` directly with no client-side page dump or link-resolution logic. `/api/v1/pages` was removed.
