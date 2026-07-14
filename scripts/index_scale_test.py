#!/usr/bin/env python3
"""Scale smoke checks for the background indexer.

Run with `uv run python scripts/index_scale_test.py`.

Optional environment:
- DATABASE_URL: enables a tb_pages convergence query.
- MIKU_EXPECTED_PAGES: expected minimum indexed page count.
- MIKU_INDEX_LOG: log file to scan for repeated "parsing/saving" operations.
- MIKU_INDEX_LOG also reports reconcile scan/parse/write/total timings emitted by
  the Rust indexer.
- MIKU_MAX_INDEX_OP_MULTIPLIER: max log operations / unique pages, default 1.5.
- MIKU_BENCH_URL: URL to probe with oha, default http://127.0.0.1:3000/p/Index.
- MIKU_BENCH_REQUESTS / MIKU_BENCH_CONCURRENCY: oha load shape.
- MIKU_SKIP_HTTP_BENCH=1: skip the oha probe.
- MIKU_BENCH_BACKEND: label printed with the benchmark result, default unknown.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import urllib.parse


def run(cmd: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    print("+ " + " ".join(cmd))
    return subprocess.run(cmd, check=False, text=True, capture_output=True, env=env)


def check_db_count() -> bool:
    database_url = os.environ.get("DATABASE_URL")
    expected = int(os.environ.get("MIKU_EXPECTED_PAGES", "0"))
    if not database_url or expected <= 0:
        print("skip: set DATABASE_URL and MIKU_EXPECTED_PAGES to check tb_pages convergence")
        return True

    parsed = urllib.parse.urlparse(database_url)
    if parsed.scheme not in {"postgres", "postgresql"}:
        print("fail: DATABASE_URL must be postgres/postgresql for tb_pages check")
        return False

    psql = shutil.which("psql")
    if not psql:
        print("skip: psql not available; cannot query tb_pages")
        return True

    result = run([psql, database_url, "-Atc", "SELECT count(*) FROM tb_pages"])
    if result.returncode != 0:
        print(result.stderr.strip())
        return False

    count = int(result.stdout.strip() or "0")
    print(f"tb_pages={count} expected>={expected}")
    return count >= expected


def check_index_log() -> bool:
    log_path = os.environ.get("MIKU_INDEX_LOG")
    if not log_path:
        print("skip: set MIKU_INDEX_LOG to check duplicate index operations")
        return True

    path_re = re.compile(r"Indexing: parsing/saving page=(.+)$")
    operations = 0
    unique_pages: set[str] = set()

    with open(log_path, encoding="utf-8", errors="replace") as log_file:
        for line in log_file:
            match = path_re.search(line)
            if match:
                operations += 1
                unique_pages.add(match.group(1).strip())

    if not unique_pages:
        print("skip: no index operations found in MIKU_INDEX_LOG")
        return True

    multiplier = float(os.environ.get("MIKU_MAX_INDEX_OP_MULTIPLIER", "1.5"))
    limit = max(len(unique_pages), int(len(unique_pages) * multiplier))
    print(f"index_ops={operations} unique_pages={len(unique_pages)} limit={limit}")
    return operations <= limit


def check_reconcile_metrics() -> bool:
    log_path = os.environ.get("MIKU_INDEX_LOG")
    if not log_path:
        return True

    summary_re = re.compile(
        r"index reconcile finished.*?scanned_files=(\d+).*?indexed_pages=(\d+)"
        r".*?unchanged_pages=(\d+).*?deleted_pages=(\d+).*?batches=(\d+)"
        r".*?walk_ms=([\d.]+).*?existing_ms=([\d.]+).*?metadata_ms=([\d.]+)"
        r".*?parse_ms=([\d.]+).*?write_ms=([\d.]+).*?total_ms=([\d.]+)"
    )
    batch_re = re.compile(
        r"index reconcile batch committed.*?batch_number=(\d+)"
        r".*?page_count=(\d+).*?write_ms=([\d.]+)"
    )
    summaries: list[tuple[str, ...]] = []
    batches: list[tuple[str, ...]] = []
    with open(log_path, encoding="utf-8", errors="replace") as log_file:
        for line in log_file:
            if match := summary_re.search(line):
                summaries.append(match.groups())
            if match := batch_re.search(line):
                batches.append(match.groups())

    if not summaries:
        print("skip: no reconcile timing metrics found in MIKU_INDEX_LOG")
        return True

    (
        scanned,
        indexed,
        unchanged,
        deleted,
        batch_count,
        walk_ms,
        existing_ms,
        metadata_ms,
        parse_ms,
        write_ms,
        total_ms,
    ) = summaries[-1]
    print(
        "reconcile_metrics="
        f"scanned:{scanned},indexed:{indexed},unchanged:{unchanged},deleted:{deleted},"
        f"batches:{batch_count},"
        f"walk_ms:{walk_ms},existing_ms:{existing_ms},metadata_ms:{metadata_ms},"
        f"parse_ms:{parse_ms},write_ms:{write_ms},total_ms:{total_ms}"
    )
    if batches:
        writes = [float(batch[2]) for batch in batches]
        print(
            f"reconcile_batch_write_ms=min:{min(writes):.2f},"
            f"median:{sorted(writes)[len(writes) // 2]:.2f},max:{max(writes):.2f}"
        )
    return True


def check_http_probe() -> bool:
    if os.environ.get("MIKU_SKIP_HTTP_BENCH") == "1":
        print("skip: MIKU_SKIP_HTTP_BENCH=1")
        return True

    oha = shutil.which("oha")
    if not oha:
        print("skip: oha not available; cannot run HTTP probe")
        return True

    url = os.environ.get("MIKU_BENCH_URL", "http://127.0.0.1:3000/p/Index")
    requests = os.environ.get("MIKU_BENCH_REQUESTS", "200")
    concurrency = os.environ.get("MIKU_BENCH_CONCURRENCY", "20")
    env = os.environ.copy()
    env.pop("NO_COLOR", None)
    result = run([oha, "-n", requests, "-c", concurrency, url], env=env)
    if result.returncode != 0:
        print(result.stderr.strip())
        return False
    print(result.stdout)
    print(f"benchmark_backend={os.environ.get('MIKU_BENCH_BACKEND', 'unknown')}")
    match = re.search(r"Success rate:\s+([0-9.]+)%", result.stdout)
    if match and float(match.group(1)) <= 0.0:
        return False
    return True


def main() -> int:
    checks = [check_db_count(), check_index_log(), check_reconcile_metrics(), check_http_probe()]
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
