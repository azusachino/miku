#!/usr/bin/env python3
"""Benchmark the core Miku HTTP API surface with uv.

Run with ``uv run python scripts/api_benchmark.py`` against a running server.
The output is intentionally stable enough to compare before/after changes.

Environment:
- MIKU_BENCH_URL: server base URL, default http://127.0.0.1:3000
- MIKU_BENCH_REQUESTS: requests per endpoint, default 20
- MIKU_BENCH_CONCURRENCY: concurrent workers, default 1
- MIKU_BENCH_NOTE: note path for note/context routes, default runtime-workflow.md
- MIKU_BENCH_TAG: tag for tag routes, default miku
"""

from __future__ import annotations

import json
import os
import statistics
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE_URL = os.environ.get("MIKU_BENCH_URL", "http://127.0.0.1:3000").rstrip("/")
REQUESTS = int(os.environ.get("MIKU_BENCH_REQUESTS", "20"))
CONCURRENCY = int(os.environ.get("MIKU_BENCH_CONCURRENCY", "1"))
NOTE = os.environ.get("MIKU_BENCH_NOTE", "runtime-workflow.md")
TAG = os.environ.get("MIKU_BENCH_TAG", "miku")
TIMEOUT = float(os.environ.get("MIKU_BENCH_TIMEOUT_SECONDS", "30"))


def endpoint_paths() -> dict[str, str]:
    encoded_note = urllib.parse.quote(NOTE, safe="/")
    encoded_tag = urllib.parse.quote(TAG, safe="")
    return {
        "health": "/healthz",
        "workspace": "/api/v1/workspace",
        "tree": "/api/v1/tree",
        "folder_tree": "/api/v1/tree?folder=adr",
        "note": f"/api/v1/notes/{encoded_note}",
        "context": f"/api/v1/note-context/{encoded_note}",
        "search": "/api/v1/search?" + urllib.parse.urlencode({"q": "miku", "scope": "all"}),
        "tags": "/api/v1/tags",
        "tag_notes": f"/api/v1/tags/{encoded_tag}/notes",
    }


def request(path: str) -> tuple[int, float]:
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(f"{BASE_URL}{path}", timeout=TIMEOUT) as response:
            response.read()
            return response.status, time.perf_counter() - started
    except urllib.error.HTTPError as error:
        error.read()
        return error.code, time.perf_counter() - started
    except urllib.error.URLError:
        return 0, time.perf_counter() - started


def percentile(values: list[float], fraction: float) -> float:
    if len(values) == 1:
        return values[0]
    return statistics.quantiles(values, n=100, method="inclusive")[int(fraction * 100) - 1]


def benchmark(path: str) -> dict[str, float | int]:
    with ThreadPoolExecutor(max_workers=CONCURRENCY) as pool:
        samples = list(pool.map(request, [path] * REQUESTS))
    durations = [duration for status, duration in samples]
    failures = sum(status < 200 or status >= 400 for status, _ in samples)
    return {
        "requests": REQUESTS,
        "concurrency": CONCURRENCY,
        "failures": failures,
        "min_ms": min(durations) * 1000,
        "p50_ms": percentile(durations, 0.50) * 1000,
        "p95_ms": percentile(durations, 0.95) * 1000,
        "max_ms": max(durations) * 1000,
    }


def main() -> int:
    paths = endpoint_paths()
    status, _ = request(paths["health"])
    if status != 200:
        raise SystemExit(f"benchmark target is unavailable at {BASE_URL} (health={status})")

    result = {
        "base_url": BASE_URL,
        "note": NOTE,
        "tag": TAG,
        "endpoints": {name: benchmark(path) for name, path in paths.items()},
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if all(stats["failures"] == 0 for stats in result["endpoints"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
