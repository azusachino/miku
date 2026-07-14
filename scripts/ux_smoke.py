"""Regular and edge-case UX smoke checks for a running Miku server."""

from __future__ import annotations

import os
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
TIMEOUT = float(os.environ.get("MIKU_UX_SMOKE_TIMEOUT_SECONDS", "10"))


def get(path: str) -> tuple[int, str, str]:
    try:
        with urllib.request.urlopen(f"{BASE_URL}{path}", timeout=TIMEOUT) as response:
            body = response.read().decode("utf-8", errors="replace")
            return response.status, response.headers.get("content-type", ""), body
    except urllib.error.HTTPError as error:
        return (
            error.code,
            error.headers.get("content-type", ""),
            error.read().decode("utf-8", errors="replace"),
        )


def expect(status: int, expected: set[int], path: str) -> None:
    if status not in expected:
        raise AssertionError(f"{path}: expected one of {sorted(expected)}, got {status}")
    print(f"ok: GET {path} -> {status}")


def encoded_page(path: str) -> str:
    return f"/p/{urllib.parse.quote(path, safe='/')}"


def main() -> int:
    expect(get("/healthz")[0], {200}, "/healthz")
    expect(get("/readyz")[0], {200, 503}, "/readyz")
    expect(get("/metrics")[0], {200}, "/metrics")

    status, content_type, favicon = get("/static/miku.svg")
    expect(status, {200}, "/static/miku.svg")
    if "image/svg+xml" not in content_type or "<svg" not in favicon:
        raise AssertionError("favicon asset is not a valid SVG response")

    page_paths = [
        "Index",
        "Changelog",
        "Features",
        "dedao-docs/README",
        "geektime-docs/README",
        "dedao-docs/docs/法律/《正义的慈悲》- 齐生解读",
    ]
    for page in page_paths:
        status, content_type, body = get(encoded_page(page))
        expect(status, {200}, encoded_page(page))
        if "text/html" not in content_type or '<link rel="icon"' not in body:
            raise AssertionError(f"{page}: rendered page is missing HTML/favicon contract")

    expect(get("/p/__miku_missing_smoke_page__")[0], {200}, "/p/missing")
    expect(get("/p/%2E%2E/%2E%2E/Cargo.toml")[0], {400, 404}, "/p/traversal")

    for scope in ("all", "title", "body"):
        status, _, body = get(f"/search?q=Miku&scope={scope}")
        expect(status, {200}, f"/search scope={scope}")
        if "Search Notes" not in body:
            raise AssertionError(f"/search scope={scope}: missing rendered search view")
    expect(get("/search?q=%E4%B8%AD%E6%96%87&scope=all")[0], {200}, "/search unicode")

    for query in ("", "Index", "Miku"):
        encoded_query = urllib.parse.urlencode({"q": query})
        expect(get(f"/api/v1/quickswitch?{encoded_query}")[0], {200}, "/api/v1/quickswitch")
    expect(get("/api/v1/nav/children?dir=dedao-docs")[0], {200}, "/api/v1/nav/children")
    expect(get("/folders/dedao-docs")[0], {200}, "/folders/dedao-docs")

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=6) as pool:
        statuses = list(pool.map(lambda page: get(encoded_page(page))[0], page_paths))
    elapsed = time.monotonic() - started
    if statuses != [200] * len(page_paths):
        raise AssertionError(f"concurrent navigation returned statuses={statuses}")
    print(f"ok: concurrent navigation pages={len(page_paths)} elapsed={elapsed:.3f}s")
    print("UX smoke passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
