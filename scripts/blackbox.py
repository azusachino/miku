#!/usr/bin/env python3
"""HTTP black-box checks for a running Miku instance."""

from __future__ import annotations

import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
PAGE = os.environ.get("MIKU_BLACKBOX_PAGE", "Index").strip("/")
QUERY = os.environ.get("MIKU_BLACKBOX_QUERY", "Index")
CONTENT_ROOT = Path(os.environ.get("MIKU_CONTENT_ROOT", "miku_docs"))
APP_CONTENT_ROOT = Path(os.environ.get("MIKU_APP_CONTENT_ROOT", "miku_docs"))
PAGE_PREFIX = os.environ.get("MIKU_BLACKBOX_PAGE_PREFIX")
READY_TIMEOUT_SECONDS = float(os.environ.get("MIKU_BLACKBOX_READY_TIMEOUT_SECONDS", "300"))


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


def validate_ready(content_type: str, body: str) -> dict[str, object]:
    if "application/json" not in content_type:
        raise AssertionError(f"/readyz: expected JSON, got {content_type}")
    health = json.loads(body)
    if health.get("status") != "ok":
        raise AssertionError(f"/readyz: unexpected payload {health}")
    return health


def wait_for_index() -> dict[str, object]:
    deadline = time.monotonic() + READY_TIMEOUT_SECONDS
    last_health: dict[str, object] = {}
    while time.monotonic() < deadline:
        status, content_type, body = get("/readyz")
        if status == 200:
            last_health = validate_ready(content_type, body)
            if last_health.get("index_ready") is True:
                return last_health
        time.sleep(1)
    raise AssertionError(
        f"/readyz: index did not become ready within {READY_TIMEOUT_SECONDS:g}s; "
        f"last payload={last_health}"
    )


def wait_for_http() -> None:
    deadline = time.monotonic() + READY_TIMEOUT_SECONDS
    while time.monotonic() < deadline:
        try:
            status, _, _ = get("/healthz")
            if status in {200, 503}:
                return
        except SystemExit:
            pass
        time.sleep(1)
    raise AssertionError(f"HTTP server did not bind within {READY_TIMEOUT_SECONDS:g}s")


def validate_page(body: str, page: str) -> None:
    if "Miku" not in body and page not in body:
        raise AssertionError(f"/p/{page}: response does not look like a rendered page")


def discover_fixture() -> tuple[str, str, str | None]:
    if os.environ.get("MIKU_BLACKBOX_PAGE"):
        page = PAGE
        folder = page.rsplit("/", 1)[0] if "/" in page else None
        return page, QUERY, folder

    candidates = sorted(
        path
        for path in CONTENT_ROOT.rglob("*.md")
        if not any(part.startswith(".") for part in path.relative_to(CONTENT_ROOT).parts)
    )
    if not candidates:
        raise AssertionError(f"no Markdown fixtures found under {CONTENT_ROOT}")

    page_path = candidates[0].relative_to(CONTENT_ROOT).with_suffix("")
    page = page_path.as_posix()
    folder = page.rsplit("/", 1)[0] if "/" in page else None
    raw = candidates[0].read_text(encoding="utf-8", errors="replace")
    words = re.findall(r"[\w\u0080-\uffff]{4,}", raw)
    query = next(
        (word for word in words if word.lower() not in {"this", "that", "with", "from"}), "Index"
    )
    return page, query, folder


def app_page_path(page: str) -> str:
    prefix = PAGE_PREFIX
    if prefix is None and CONTENT_ROOT != APP_CONTENT_ROOT:
        try:
            prefix = CONTENT_ROOT.relative_to(APP_CONTENT_ROOT).as_posix()
        except ValueError:
            prefix = None
    return f"{prefix}/{page}" if prefix else page


def stress_navigation() -> None:
    pages = [
        path.relative_to(CONTENT_ROOT).with_suffix("").as_posix()
        for path in sorted(CONTENT_ROOT.rglob("*.md"))
        if not any(part.startswith(".") for part in path.relative_to(CONTENT_ROOT).parts)
    ][:12]
    if not pages:
        raise AssertionError(f"no Markdown navigation pages found under {CONTENT_ROOT}")

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=min(8, len(pages))) as pool:
        results = list(
            pool.map(
                lambda page: get(f"/p/{urllib.parse.quote(page, safe='/')}"),
                pages,
            )
        )
    elapsed = time.monotonic() - started
    for page, (status, _, _) in zip(pages, results, strict=True):
        expect(status, 200, f"/p/{page} navigation stress")
    print(f"ok: concurrent navigation pages={len(pages)} elapsed={elapsed:.3f}s")


def discover_tag() -> str | None:
    pattern = re.compile(r"(?<!\w)#([\w\u0080-\uffff][\w\u0080-\uffff_/-]*)")
    for path in CONTENT_ROOT.rglob("*.md"):
        if any(part.startswith(".") for part in path.relative_to(CONTENT_ROOT).parts):
            continue
        match = pattern.search(path.read_text(encoding="utf-8", errors="replace"))
        if match:
            return match.group(1)
    return None


def main() -> int:
    page, query_text, folder = discover_fixture()
    app_page = app_page_path(page)
    app_folder = app_page.rsplit("/", 1)[0] if "/" in app_page else None
    tag = discover_tag()
    print(
        f"fixture: page={page} app_page={app_page} query={query_text!r} "
        f"folder={folder!r} tag={tag!r}"
    )

    wait_for_http()

    encoded_page = urllib.parse.quote(app_page, safe="/")

    # The page source must remain available before the background indexer is
    # ready. This is the regression path for a fresh/large Turso projection.
    status, _, body = get(f"/p/{encoded_page}")
    expect(status, 200, f"/p/{page} before index ready")
    validate_page(body, page)
    mention_query = urllib.parse.urlencode({"q": "Miku", "scope": "body"})
    status, _, _ = get(f"/search?{mention_query}")
    expect(status, 200, "/search?query=Miku&scope=body before index ready")

    health = wait_for_index()
    print(f"ok: backend ready capabilities={health.get('capabilities', {})}")
    stress_navigation()

    status, _, body = get("/metrics")
    expect(status, 200, "/metrics")
    if "miku_http_requests_total" not in body or "miku_index_ready 1" not in body:
        raise AssertionError("/metrics: missing request or index readiness metrics")

    status, _, _ = get("/")
    expect(status, 200, "/")

    status, _, body = get(f"/p/{encoded_page}")
    expect(status, 200, f"/p/{page}")
    validate_page(body, page)

    status, _, body = get(f"/p/{encoded_page}/edit")
    expect(status, 200, f"/p/{page}/edit")
    if "textarea" not in body:
        raise AssertionError(f"/p/{page}/edit: missing editor textarea")

    query = urllib.parse.urlencode({"q": query_text})
    status, _, _ = get(f"/api/v1/quickswitch?{query}")
    expect(status, 200, "/api/v1/quickswitch")

    status, _, body = get(f"/search?{query}&scope=all")
    expect(status, 200, "/search")
    if query_text.lower() not in body.lower():
        raise AssertionError("/search: response does not contain the discovered query")

    status, _, body = get("/tags")
    expect(status, 200, "/tags")
    if tag:
        encoded_tag = urllib.parse.quote(tag, safe="")
        status, _, _ = get(f"/tags/{encoded_tag}")
        expect(status, 200, f"/tags/{tag}")

    if app_folder:
        encoded_folder = urllib.parse.quote(app_folder, safe="/")
        status, _, _ = get(f"/folders/{encoded_folder}")
        expect(status, 200, f"/folders/{app_folder}")
        status, _, _ = get(f"/api/v1/nav/children?dir={encoded_folder}")
        expect(status, 200, "/api/v1/nav/children")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, json.JSONDecodeError) as error:
        print(f"black-box failure: {error}", file=sys.stderr)
        raise SystemExit(1) from error
