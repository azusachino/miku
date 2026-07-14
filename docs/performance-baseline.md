# Indexing performance baseline

Captured 2026-07-14 from the real `miku_docs` corpus and local Turso 0.7 runtime. These are baseline observations, not release targets.

## Corpus

| Corpus          | Markdown files | Raw size |
| --------------- | -------------: | -------: |
| `dedao-docs`    |          4,057 |   147 MB |
| `geektime-docs` |         10,520 |   187 MB |
| Combined        |         14,577 |   334 MB |

## Observed behavior before the responsive-read fix

- A fresh native Turso FTS rebuild produced frequent Tantivy commit and garbage collection logs and took long enough to make startup feel hung.
- The durable projection reached roughly 600 MB during the rebuild. Storage size was accepted as a trade-off; startup latency was the problem.
- During FTS commits, `GET /p/Index` waited about 15.26 seconds and returned HTTP 500 with `concurrent use forbidden` / FTS parse errors.
- The outer Rust reconcile batch was 128 pages, but the observed Tantivy logs showed much smaller effective commits. SQL batching therefore did not prove Tantivy batching.

## Current behavior after the read-path fix

- While `/api/health` reported `index_ready=false`, `GET /p/Index` returned HTML in about 0.22 seconds.
- Busy Turso search falls back to the in-process projection; the page may show temporarily stale index metadata, but it remains readable.
- Same-directory restart reuses committed projections and skips unchanged files by mtime.

## Tunable benchmark matrix

The reconcile batch defaults to 512. Compare the write/event overhead with the same corpus and fresh index path:

```bash
for size in 128 512 1000; do
  MIKU_RECONCILE_BATCH_SIZE=$size \
  MIKU_INDEX_PATH="/tmp/miku-$size.turso" \
  MIKU_BENCH_BACKEND=turso \
  make run
done
```

In a second shell, probe page availability during rebuild:

```bash
MIKU_BENCH_REQUESTS=200 MIKU_BENCH_CONCURRENCY=20 make benchmark
```

Record at minimum: time until `index_ready=true`, effective Tantivy commit frequency, page-request success rate/latency while not ready, final index size, and restart convergence time. Do not compare
only final database size.

## Implemented batching change

- Reconciliation batches default to 512 and can be set with `MIKU_RECONCILE_BATCH_SIZE`.
- Each committed batch emits one bulk SSE refresh rather than one event per page.
- A no-op periodic reconcile emits no refresh event.
- Durable writes remain serialized through the Turso backend; batching does not weaken restart or source-of-truth guarantees.

The next likely performance boundary is separate Tantivy construction or a deferred body index. The current measurements should be refreshed before choosing that larger architectural change.
