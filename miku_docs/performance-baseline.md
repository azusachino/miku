# Indexing performance baseline

This document records the current 0.0.2 SQLite/SQLx profile. Historical backend measurements remain archival context.

## Corpus

| Corpus          | Markdown files | Raw size |
| --------------- | -------------: | -------: |
| `dedao-docs`    |          4,057 |   147 MB |
| `geektime-docs` |         10,520 |   187 MB |
| Combined        |         14,577 |   334 MB |

## Verified current properties

- The local durable index is SQLite via SQLx with WAL mode, foreign keys, a five-second busy timeout, and SQLite FTS5.
- The index is disposable and rebuilt from `miku_docs/**/*.md`.
- Page, link, tag, alias, mention, and FTS writes are transactional at the backend boundary.
- HTTP reads use the backend-neutral `IndexReader` contract; the filesystem remains the source of truth.
- The default backend is selected with `MIKU_INDEX_BACKEND=memory` and uses the rebuildable memory/Tantivy projection; SQLite remains available with `MIKU_INDEX_BACKEND=sqlite`.

## Dependency closure

The migration reduced the root package's normal dependency tree from 403 unique packages to 254, a reduction of 149 packages (36.9%). The complete lockfile resolution fell from 482 package records to
312, a reduction of 170 records (35.3%). Both figures count the complete resolved dependency set, including transitive packages.

Reproduce the current measurements with:

```bash
cargo tree -p miku --edges normal --prefix none | sort -u | wc -l
rg '^name = ' Cargo.lock | wc -l
```

## Runtime benchmark plan

Use a fresh SQLite path for each run:

```bash
for size in 128 512 1000; do
  MIKU_RECONCILE_BATCH_SIZE=$size \
  MIKU_INDEX_PATH="/tmp/miku-$size.sqlite" \
  MIKU_INDEX_BACKEND=sqlite \
  make run
done
```

Probe page availability and search while reconciliation runs:

```bash
MIKU_BENCH_REQUESTS=200 MIKU_BENCH_CONCURRENCY=20 make benchmark
```

The indexer emits reconcile phase timings, including directory traversal, metadata checks, parsing, backend writes, and total elapsed time. Use `oha` for HTTP latency distributions and `hyperfine` for
repeatable read-path comparisons. Record at minimum:

- time until `index_ready=true`;
- indexed, unchanged, and failed page counts;
- reconcile and backend-write p50/p95 where available;
- final SQLite file plus `-wal`/`-shm` sizes;
- page-request success rate and latency during initial indexing;
- restart convergence time.

No SQLite throughput or latency target is claimed until this benchmark is run against the current profile.

## Current batching behavior

- Reconciliation batches default to 512 and are configurable with `MIKU_RECONCILE_BATCH_SIZE`.
- SQLite batch writes commit through one backend transaction.
- A no-op periodic reconcile emits no refresh event.
- SQLite WAL permits concurrent readers while the indexer remains the single writer.
