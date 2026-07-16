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
            return response.status, response.headers.get("content-type", ""), response.read().decode(
                "utf-8", errors="replace"
            )
    except urllib.error.HTTPError as error:
        return error.code, error.headers.get("content-type", ""), error.read().decode(
            "utf-8", errors="replace"
        )


def expect(status: int, expected: int, path: str) -> None:
    if status != expected:
        raise AssertionError(f"{path}: expected HTTP {expected}, got {status}")
    print(f"ok: GET {path} -> {status}")


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
            health = json.loads(body)
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
    tree = json_get("/api/v1/tree")
    content_root = Path(os.environ.get("MIKU_CONTENT_ROOT", "miku_docs"))
    candidates = sorted(content_root.rglob("*.md"))
    if not candidates:
        raise AssertionError("workspace contains no Markdown source files")
    note_id = candidates[0].relative_to(content_root).as_posix()
    encoded_id = urllib.parse.quote(note_id, safe="")

    note = json_get(f"/api/v1/notes/{encoded_id}")
    if note.get("path") != note_id:
        raise AssertionError(f"note identity mismatch: {note}")
    context = json_get(f"/api/v1/note-context/{encoded_id}")
    if "backlinks" not in context:
        raise AssertionError(f"note context missing graph fields: {context}")

    query = urllib.parse.urlencode({"q": note.get("title", ""), "limit": 5})
    search = json_get(f"/api/v1/search?{query}")
    if not search.get("results"):
        raise AssertionError(f"search returned no result for {note.get('title')!r}")

    tags = json_get("/api/v1/tags")
    if tags:
        tag = urllib.parse.quote(tags[0]["tag"], safe="")
        json_get(f"/api/v1/tags/{tag}/notes")
    print("ok: default composed API workspace/tree/note/context/search/tags")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
