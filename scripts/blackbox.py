#!/usr/bin/env python3
"""HTTP black-box checks for Miku's versioned read API."""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
READY_TIMEOUT_SECONDS = float(os.environ.get("MIKU_BLACKBOX_READY_TIMEOUT_SECONDS", "300"))


def get(path: str) -> tuple[int, str, str]:
    try:
        with urllib.request.urlopen(f"{BASE_URL}{path}", timeout=60) as response:
            body = response.read().decode("utf-8", errors="replace")
            return response.status, response.headers.get("content-type", ""), body
    except urllib.error.HTTPError as error:
        return (
            error.code,
            error.headers.get("content-type", ""),
            error.read().decode("utf-8", errors="replace"),
        )


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


def json_get(path: str) -> object:
    status, content_type, body = get(path)
    expect(status, 200, path)
    if "application/json" not in content_type:
        raise AssertionError(f"{path}: expected JSON, got {content_type}")
    return json.loads(body)


def wait_for_ready() -> dict[str, object]:
    deadline = time.monotonic() + READY_TIMEOUT_SECONDS
    while time.monotonic() < deadline:
        status, _, body = get("/readyz")
        if status == 200:
            health = validate_ready("application/json", body)
            if health.get("index_ready") is True:
                return health
        time.sleep(1)
    raise AssertionError("/readyz: index did not become ready")


def main() -> int:
    root = json_get("/")
    if root.get("api") != "/api/v1":
        raise AssertionError(f"/: unexpected API root {root}")
    json_get("/healthz")
    health = wait_for_ready()
    print(f"ok: ready capabilities={health.get('capabilities', {})}")

    workspace = json_get("/api/v1/workspace")
    if not workspace.get("note_count", 0):
        raise AssertionError("workspace contains no Markdown notes")

    json_get("/api/v1/tree")
    json_get("/api/openapi.json")

    content_root = Path(os.environ.get("MIKU_CONTENT_ROOT", "miku_docs"))
    candidates = sorted(content_root.rglob("*.md"))
    if not candidates:
        raise AssertionError("workspace contains no Markdown source files")

    note_id = candidates[0].relative_to(content_root).as_posix()
    encoded_id = urllib.parse.quote(note_id, safe="")

    # Happy path note reads
    note = json_get(f"/api/v1/notes/{encoded_id}")
    if note.get("path") != note_id:
        raise AssertionError(f"note identity mismatch: {note}")

    start_time = time.monotonic()
    context = json_get(f"/api/v1/note-context/{encoded_id}")
    context_latency_ms = (time.monotonic() - start_time) * 1000
    if "backlinks" not in context or "outgoing" not in context:
        raise AssertionError(f"note context missing graph fields: {context}")
    if context_latency_ms > 200:
        raise AssertionError(f"note-context latency degraded: {context_latency_ms:.2f}ms > 200ms")
    print(f"ok: note-context latency={context_latency_ms:.2f}ms (<200ms)")

    json_get(f"/api/v1/note-children/{encoded_id}")

    # Vendored geektime-docs note read and context verification
    geektime_candidates = [p for p in candidates if "geektime-docs" in p.parts]
    if geektime_candidates:
        geektime_note_id = geektime_candidates[0].relative_to(content_root).as_posix()
        encoded_geektime_id = urllib.parse.quote(geektime_note_id, safe="")
        geektime_note = json_get(f"/api/v1/notes/{encoded_geektime_id}")
        if not geektime_note.get("title"):
            raise AssertionError(f"geektime note missing title: {geektime_note}")
        geektime_context = json_get(f"/api/v1/note-context/{encoded_geektime_id}")
        if "backlinks" not in geektime_context:
            raise AssertionError("geektime note context missing backlinks")
        print(f"ok: vendored geektime-docs note read and context verified ({geektime_note_id})")

    # Evil / Edge cases: 404 for missing note
    missing_status, _, _ = get("/api/v1/notes/non_existent_note_99999.md")
    expect(missing_status, 404, "/api/v1/notes/non_existent_note_99999.md")
    missing_ctx_status, _, _ = get("/api/v1/note-context/non_existent_note_99999.md")
    expect(missing_ctx_status, 404, "/api/v1/note-context/non_existent_note_99999.md")

    # Search: happy, scopes, special chars, limit clamping, performance
    for scope in ["title", "content", "body", "all", "unknown_scope"]:
        query = urllib.parse.urlencode({"q": note.get("title", ""), "limit": 5, "scope": scope})
        json_get(f"/api/v1/search?{query}")

    # Special char escaping in search query
    special_query = urllib.parse.urlencode({"q": "%\\_special*", "limit": 5})
    json_get(f"/api/v1/search?{special_query}")

    # Limit clamping
    large_limit_query = urllib.parse.urlencode({"q": "Miku", "limit": 99999})
    search_clamped = json_get(f"/api/v1/search?{large_limit_query}")
    if len(search_clamped.get("results", [])) > 100:
        raise AssertionError("search limit was not clamped to 100")

    # Search performance check
    start_time = time.monotonic()
    search_perf_query = urllib.parse.urlencode({"q": "Miku", "limit": 20})
    json_get(f"/api/v1/search?{search_perf_query}")
    search_latency_ms = (time.monotonic() - start_time) * 1000
    if search_latency_ms > 200:
        raise AssertionError(f"search latency degraded: {search_latency_ms:.2f}ms > 200ms")
    print(f"ok: search latency={search_latency_ms:.2f}ms (<200ms)")

    # Tags & Tag Notes
    tags = json_get("/api/v1/tags")
    if tags:
        tag = urllib.parse.quote(tags[0]["tag"], safe="")
        json_get(f"/api/v1/tags/{tag}/notes")

    # Edge case: non-existent tag returns empty list 200
    empty_tag_notes = json_get("/api/v1/tags/non_existent_tag_xyz/notes")
    if empty_tag_notes != []:
        raise AssertionError("non-existent tag notes must return empty list")

    print(
        "ok: full API coverage (happy, evil 404/escaping, limit-clamping, latency <200ms) verified"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
