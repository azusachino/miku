#!/usr/bin/env python3
"""HTTP black-box checks for a running Miku instance."""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
PAGE = os.environ.get("MIKU_BLACKBOX_PAGE", "Index").strip("/")
QUERY = os.environ.get("MIKU_BLACKBOX_QUERY", "Index")


def get(path: str) -> tuple[int, str, str]:
    url = f"{BASE_URL}{path}"
    try:
        with urllib.request.urlopen(url, timeout=5) as response:
            return (
                response.status,
                response.headers.get("content-type", ""),
                response.read().decode("utf-8", errors="replace"),
            )
    except urllib.error.HTTPError as error:
        return (
            error.code,
            error.headers.get("content-type", ""),
            error.read().decode("utf-8", errors="replace"),
        )
    except urllib.error.URLError as error:
        raise SystemExit(f"black-box connection failed for {url}: {error.reason}") from error


def expect(status: int, expected: int, path: str) -> None:
    if status != expected:
        raise AssertionError(f"{path}: expected HTTP {expected}, got {status}")
    print(f"ok: GET {path} -> {status}")


def validate_health(content_type: str, body: str) -> dict[str, object]:
    if "application/json" not in content_type:
        raise AssertionError(f"/api/health: expected JSON, got {content_type}")
    health = json.loads(body)
    if health.get("status") != "ok":
        raise AssertionError(f"/api/health: unexpected payload {health}")
    return health


def validate_page(body: str, page: str) -> None:
    if "Miku" not in body and page not in body:
        raise AssertionError(f"/p/{page}: response does not look like a rendered page")


def main() -> int:
    status, content_type, body = get("/api/health")
    expect(status, 200, "/api/health")
    health = validate_health(content_type, body)
    print(f"ok: backend capabilities={health.get('capabilities', {})}")

    status, _, _ = get("/")
    expect(status, 200, "/")

    status, _, body = get(f"/p/{urllib.parse.quote(PAGE)}")
    expect(status, 200, f"/p/{PAGE}")
    validate_page(body, PAGE)

    query = urllib.parse.urlencode({"q": QUERY})
    status, _, _ = get(f"/api/quickswitch?{query}")
    expect(status, 200, "/api/quickswitch")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, json.JSONDecodeError) as error:
        print(f"black-box failure: {error}", file=sys.stderr)
        raise SystemExit(1) from error
