# Vault model experiments

These probes are deliberately outside the Miku crates and frontend. They compare the persistence boundaries suggested by the vendored projects:

- `memory`: filesystem plus an in-process projection; restart reparses everything.
- `files-cache`: filesystem remains authoritative and a disposable manifest cache records file identity and parsed metadata, following Tolaria's cache boundary.
- `trilium-graph`: SQLite owns notes, placements, and content, following Trilium's `notes`/`branches` split. Markdown import/export is an adapter concern.

The proposed Miku architecture is intentionally narrower than that candidate set: SQLite/Postgres are durable projection backends, while Memory/Valkey are hot-read backends. RocksDB is rejected: it
adds another embedded persistence model without providing SQLite's relational graph and FTS advantages.

Run:

```bash
uv run python experiments/compare_vault_models.py --files 11000
```

The benchmark creates a temporary corpus, measures cold startup, restart, one-file change, and one-file read, then checks the result invariants. It does not touch `miku_docs/` or the application
runtime.

The expected decision question is not “which benchmark is fastest?” It is:

1. Can the user-owned Markdown files remain the authority?
2. Can restart avoid reparsing 11k files?
3. Can a single external edit converge without a full rebuild?
4. Do we need Trilium's multiple-parent graph as a first-class write model?

Run the ownership and invalidation proof separately:

```bash
uv run python experiments/hybrid_projection_probe.py
```

This proves the contract that must remain true for both Memory and Valkey hot backends: durable commit first, atomic cache publication second, revision-checked read fallback, and no partially
committed values visible to readers.
