"""Regular and edge-case UX smoke checks for a running Miku server."""

from __future__ import annotations

import json
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

    status, content_type, body = get("/api/v1/workspace")
    expect(status, {200}, "/api/v1/workspace")
    workspace = json.loads(body)
    if workspace["root"] != "miku_docs" or workspace["note_count"] <= 0:
        raise AssertionError(f"workspace bootstrap is not corpus-backed: {workspace}")

    status, _, body = get("/api/v1/tree")
    expect(status, {200}, "/api/v1/tree")
    if not json.loads(body)["nodes"]:
        raise AssertionError("workspace tree has no root nodes")
    expect(get("/api/v1/tree?folder=dedao-docs")[0], {200}, "/api/v1/tree folder")

    status, content_type, body = get("/api/v1/notes/Index.md")
    expect(status, {200}, "/api/v1/notes/Index.md")
    if "application/json" not in content_type or json.loads(body)["title"] != "Index":
        raise AssertionError("note API did not return the Index contract")

    search_queries = {"all": "Miku", "title": "Index", "content": "Miku"}
    for scope, query in search_queries.items():
        query_string = urllib.parse.urlencode({"q": query, "scope": scope})
        status, _, body = get(f"/api/v1/search?{query_string}")
        expect(status, {200}, f"/api/v1/search scope={scope}")
        if not json.loads(body)["results"]:
            raise AssertionError(f"search scope={scope} returned no corpus results")
    expect(get("/api/v1/tags")[0], {200}, "/api/v1/tags")
    expect(get("/api/openapi.json")[0], {200}, "/api/openapi.json")

    page_paths = ["Index.md", "Changelog.md", "Features.md", "Usage.md"]

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=6) as pool:
        statuses = list(
            pool.map(lambda page: get(f"/api/v1/notes/{urllib.parse.quote(page)}")[0], page_paths)
        )
    elapsed = time.monotonic() - started
    if statuses != [200] * len(page_paths):
        raise AssertionError(f"concurrent navigation returned statuses={statuses}")
    print(f"ok: concurrent navigation pages={len(page_paths)} elapsed={elapsed:.3f}s")
    print("UX smoke passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
