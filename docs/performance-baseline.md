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

- While `/readyz` reported `index_ready=false`, `GET /p/Index` returned HTML in about 0.22 seconds.
- Busy Turso search falls back to the in-process projection; the page may show temporarily stale index metadata, but it remains readable.
- Same-directory restart reuses committed projections and skips unchanged files by mtime.

## Measured bulk-FTS change

The initial per-page FTS maintenance path was unacceptable on the real corpus: a 150-second bounded run committed only 512 pages, or about 3.41 pages/sec. The Turso file reached 52 MB while the first
transaction was still in flight.

After deferring FTS maintenance until the bulk projection is complete, the same 150-second bounded run committed all 14,311 pages with 124.9 MB of payload in a 542 MB Turso file. This is at least 95.4
pages/sec, approximately 28x the old observed rate. The run completed before the timeout; an exact ready-time sample should be refreshed after the next benchmark pass.

The new path drops the FTS index once before multi-page reconcile writes, persists the page projection without per-document FTS maintenance, and rebuilds the FTS index once at the end. Search uses the
in-memory projection while FTS is suspended.

## Tunable benchmark matrix

The reconcile batch defaults to 512. Compare the write/event overhead with the same corpus and fresh index path:

```bash
for size in 128 512 1000; do
  MIKU_RECONCILE_BATCH_SIZE=$size \
  MIKU_INDEX_PATH="/tmp/miku-$size.sqlite" \
  MIKU_BENCH_BACKEND=sqlite \
  make run
done
```

In a second shell, probe page availability during rebuild:

```bash
MIKU_BENCH_REQUESTS=200 MIKU_BENCH_CONCURRENCY=20 make benchmark
```

With the server log redirected, `scripts/index_scale_test.py` now extracts the reconcile timing fields and per-batch write distribution:

```bash
# shell 1
RUST_LOG=info make run 2>&1 | tee /tmp/miku-index.log

# shell 2, after the server is responding
MIKU_INDEX_LOG=/tmp/miku-index.log \
  MIKU_BENCH_BACKEND=sqlite make benchmark
```

The Rust indexer emits `startup index reconcile ready elapsed_ms=...`, one `index reconcile batch committed ... write_ms=...` event per batch, and an
`index reconcile finished ... indexed_pages=... unchanged_pages=... walk_ms=... existing_ms=... metadata_ms=... parse_ms=... write_ms=... total_ms=...` summary. The phase fields separate directory
traversal, loading the durable projection into memory, source metadata checks, changed-file projection building, and backend writes. A same-directory no-op restart should show all source files under
`unchanged_pages` and `indexed_pages=0`; it still walks the tree and stats files, but it does not reread or reparse unchanged Markdown. Use `oha` for the HTTP latency distribution and `hyperfine` for
repeatable read-path comparisons while the server is running:

```bash
hyperfine --warmup 2 --runs 10 'curl -fsS http://127.0.0.1:3000/p/Index'
MIKU_BENCH_REQUESTS=1000 MIKU_BENCH_CONCURRENCY=32 make benchmark
xh get http://127.0.0.1:3000/metrics
```

Changed Markdown projection is bounded across blocking workers with `MIKU_PARSE_CONCURRENCY` (default `8`); Turso writes remain serialized. Compare `MIKU_PARSE_CONCURRENCY=1`, `4`, and `8` using the
same fresh index path and record `parse_ms` separately from `write_ms` before increasing concurrency.

`/metrics` is deliberately implemented in the application using the Prometheus text exposition format, without adding a metrics exporter to the Rust dependency graph. It exposes process uptime, index
readiness, request count, cumulative response duration, and response-duration buckets in microseconds. Scrape it during a cold rebuild and while running `oha` to separate startup/indexing cost from
HTTP tail latency.

Record at minimum: time until `index_ready=true`, each reconcile phase, per-batch write p50/p95 if available, effective Tantivy commit frequency, page-request success rate/latency while not ready,
final index size, and restart convergence time. Do not compare only final database size.

## Implemented batching change

- Reconciliation batches default to 512 and can be set with `MIKU_RECONCILE_BATCH_SIZE`.
- Each committed batch emits one bulk SSE refresh rather than one event per page.
- A no-op periodic reconcile emits no refresh event.
- Durable writes remain serialized through the Turso backend; batching does not weaken restart or source-of-truth guarantees.

The next likely performance boundary is separate Tantivy construction or a deferred body index. The current measurements should be refreshed before choosing that larger architectural change.
