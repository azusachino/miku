"""Long-running navigation/search soak test for a running Miku server."""

from __future__ import annotations

import os
import statistics
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
DURATION = float(os.environ.get("MIKU_UX_SOAK_SECONDS", "60"))
TIMEOUT = float(os.environ.get("MIKU_UX_SOAK_TIMEOUT_SECONDS", "10"))
MAX_P95 = float(os.environ.get("MIKU_UX_SOAK_MAX_P95_SECONDS", "5"))

PAGE_PATHS = (
    "Index",
    "Changelog",
    "Features",
    "dedao-docs/README",
    "geektime-docs/README",
    "dedao-docs/docs/法律/《正义的慈悲》- 齐生解读",
    "__miku_soak_new_page__",
)


def get(path: str) -> tuple[int, float]:
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


def page_path(path: str) -> str:
    return f"/p/{urllib.parse.quote(path, safe='/')}"


def action_paths(round_number: int) -> list[str]:
    page = PAGE_PATHS[round_number % len(PAGE_PATHS)]
    search_scope = ("all", "title", "body")[round_number % 3]
    search_query = ("Miku", "Index", "中文")[round_number % 3]
    return [
        page_path(page),
        f"/search?{urllib.parse.urlencode({'q': search_query, 'scope': search_scope})}",
        f"/api/v1/quickswitch?{urllib.parse.urlencode({'q': search_query})}",
        "/api/v1/nav/children?dir=dedao-docs",
        "/folders/dedao-docs",
    ]


def p95(values: list[float]) -> float:
    if not values:
        return 0.0
    return (
        statistics.quantiles(values, n=20, method="inclusive")[-1] if len(values) > 1 else values[0]
    )


def main() -> int:
    health_status, _ = get("/healthz")
    if health_status != 200:
        raise SystemExit(
            f"UX soak target is unavailable at {BASE_URL}/healthz (status={health_status}); "
            "start Miku first"
        )

    started = time.monotonic()
    samples: list[tuple[int, float]] = []
    rounds = 0
    next_report = started + 10
    interrupted = False
    try:
        with ThreadPoolExecutor(max_workers=5) as pool:
            while time.monotonic() - started < DURATION:
                samples.extend(pool.map(get, action_paths(rounds)))
                rounds += 1
                now = time.monotonic()
                if now >= next_report:
                    failures = sum(status < 200 or status >= 400 for status, _ in samples)
                    print(
                        f"progress elapsed={now - started:.0f}s rounds={rounds} "
                        f"requests={len(samples)} failures={failures}",
                        flush=True,
                    )
                    next_report = now + 10
    except KeyboardInterrupt:
        interrupted = True
        print("interrupted: reporting partial soak metrics", flush=True)

    latencies = [elapsed for status, elapsed in samples]
    failures = [(status, elapsed) for status, elapsed in samples if status < 200 or status >= 400]
    split = max(1, len(latencies) // 10)
    first_p95 = p95(latencies[:split])
    last_p95 = p95(latencies[-split:])
    print(
        f"UX soak interrupted={str(interrupted).lower()} "
        f"duration={time.monotonic() - started:.1f}s rounds={rounds} "
        f"requests={len(samples)} failures={len(failures)} "
        f"first_p95={first_p95:.3f}s last_p95={last_p95:.3f}s "
        f"max={max(latencies, default=0):.3f}s"
    )
    if interrupted:
        return 130
    if failures:
        raise AssertionError(f"UX soak observed failed HTTP statuses: {failures[:5]}")
    if last_p95 > MAX_P95:
        raise AssertionError(f"UX soak last-window p95 {last_p95:.3f}s exceeds {MAX_P95:.3f}s")
    print("UX soak passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
