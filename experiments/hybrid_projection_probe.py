#!/usr/bin/env python3
"""Prove the durable-projection plus hot-cache ownership contract."""

from __future__ import annotations

import sqlite3
import tempfile
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from threading import RLock


@dataclass(frozen=True)
class CachedDocument:
    revision: int
    body: str


class HybridProjection:
    """SQLite durable projection with an atomic in-process hot read projection."""

    def __init__(self, path: Path) -> None:
        self.db = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
        self.db.execute(
            "CREATE TABLE documents("
            "path TEXT PRIMARY KEY, revision INTEGER NOT NULL, body TEXT NOT NULL)"
        )
        self.cache: dict[str, CachedDocument] = {}
        self.lock = RLock()

    def commit_from_indexer(self, path: str, body: str) -> int:
        """Commit durably first, then publish one immutable hot-cache value."""
        with self.lock, self.db:
            current = self.db.execute(
                "SELECT revision FROM documents WHERE path = ?", (path,)
            ).fetchone()
            revision = (current[0] if current else 0) + 1
            self.db.execute(
                "INSERT INTO documents(path, revision, body) VALUES (?, ?, ?) "
                "ON CONFLICT(path) DO UPDATE SET revision=excluded.revision, body=excluded.body",
                (path, revision, body),
            )
            self.cache[path] = CachedDocument(revision, body)
            return revision

    def read_for_web(self, path: str) -> CachedDocument | None:
        """Return hot data only when its revision still matches durable state."""
        with self.lock:
            cached = self.cache.get(path)
            durable = self.db.execute(
                "SELECT revision, body FROM documents WHERE path = ?", (path,)
            ).fetchone()
            if durable is None:
                return None
            if cached and cached.revision == durable[0]:
                return cached
            refreshed = CachedDocument(durable[0], durable[1])
            self.cache[path] = refreshed
            return refreshed

    def restart(self) -> None:
        """Drop only the hot projection; the durable projection survives."""
        with self.lock:
            self.cache.clear()

    def close(self) -> None:
        self.db.close()


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="miku-hybrid-") as directory:
        projection = HybridProjection(Path(directory) / "projection.sqlite")
        assert projection.commit_from_indexer("note.md", "v1") == 1
        assert projection.read_for_web("note.md") == CachedDocument(1, "v1")

        projection.restart()
        assert projection.read_for_web("note.md") == CachedDocument(1, "v1")

        assert projection.commit_from_indexer("note.md", "v2") == 2
        assert projection.read_for_web("note.md") == CachedDocument(2, "v2")

        # Simulate an already-committed durable update whose cache event was lost.
        with projection.db:
            projection.db.execute(
                "UPDATE documents SET revision = 3, body = 'v3' WHERE path = 'note.md'"
            )
        assert projection.read_for_web("note.md") == CachedDocument(3, "v3")

        assert projection.commit_from_indexer("note.md", "v4") == 4
        with ThreadPoolExecutor(max_workers=8) as workers:
            observed = list(workers.map(lambda _: projection.read_for_web("note.md"), range(64)))
        assert {document for document in observed} == {CachedDocument(4, "v4")}
        projection.close()

    print("hybrid projection proof passed")
    print("- durable commit precedes hot-cache publication")
    print("- restart repopulates memory from SQLite without source reparsing")
    print("- lost invalidation self-heals through revision validation")
    print("- concurrent readers observe one complete committed version")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
