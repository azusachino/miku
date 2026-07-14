# Architecture Decision Records (verified)

This folder holds **only verified ADRs** — decisions that have been accepted and
are safe to build against. Proposed or under-discussion decisions stay in
`docs/decisions.md`; once verified, an ADR **graduates** here as its own file.

## Convention

- **One file per ADR**, named `NNNN-kebab-title.md` (zero-padded, e.g.
  `0001-fts-english.md`). The number is permanent and never reused.
- Every file starts with a status header:

  ```markdown
  # ADR-NNNN — <Title>

  - **Status:** Accepted        <!-- Accepted | Superseded by ADR-NNNN -->
  - **Date:** YYYY-MM-DD
  - **Mirror:** asobi `miku:decision:<slug>`
  ```

- Body sections: **Decision**, **Why**, **Trade-offs / Rejected**. Keep it the
  decision and its forces — not code.
- **Superseding, never editing.** A verified ADR is immutable. To reverse one,
  add a new ADR and set the old file's status to `Superseded by ADR-NNNN`
  (mirror the `supersedes` link in asobi).

## Lifecycle

```
docs/decisions.md (proposed)  →  verified  →  docs/adr/NNNN-*.md (Accepted)
```

A decision is **verified** when it is accepted by the maintainer, consistent with
`architecture.md`'s core invariant, and has no open questions blocking
implementation. Implementation builds against this folder.

## Index

| ADR | Title | Status | Accepted |
|---|---|---|---|
| [0001](0001-fts-english.md) | Full-text search (Postgres english FTS) | Accepted | 2026-06-26 |
| [0002](0002-markdown-wikilink-grammar.md) | Markdown & wikilink grammar (comrak) | Accepted | 2026-06-26 |
| [0003](0003-write-conflicts-auth.md) | Write conflicts & auth | Accepted | 2026-06-26 |
| [0004](0004-rename-delete-assets.md) | Rename / delete & assets | Accepted | 2026-06-26 |
| [0005](0005-nav-explorer.md) | Navigation explorer (folder/file tree) | Accepted | 2026-06-26 |
| [0006](0006-watcher-at-scale.md) | Filesystem watcher at scale | Accepted | 2026-06-26 |
| [0007](0007-frontend-rendering.md) | Frontend rendering & client-JS budget | Accepted | 2026-06-26 |
| [0008](0008-theme-switching.md) | Theme switching (palette × mode) | Accepted | 2026-06-26 |
| [0009](0009-index-store-composition.md) | Index store and cache composition | Accepted | 2026-07-14 |
| [0010](0010-workspace-crate-boundaries.md) | Cargo workspace and crate boundaries | Accepted | 2026-07-14 |
| [0011](0011-local-tier-turso-index.md) | Local deployment tier with Turso index | Accepted | 2026-07-14 |
| [0012](0012-scale-tier-postgres-valkey.md) | Scale deployment tier with Postgres and Valkey | Accepted | 2026-07-14 |
| [0013](0013-crates-io-release-surface.md) | crates.io release surface | Accepted | 2026-07-14 |

Staged (proposed, not yet verified) in `docs/decisions.md`: none outstanding.
