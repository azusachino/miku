#!/usr/bin/env python3
"""Compare restart and single-edit behavior for three note persistence models."""

from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Measurement:
    name: str
    seconds: float
    parsed: int
    visible: int


def timed(name: str, operation) -> Measurement:
    started = time.perf_counter()
    parsed, visible = operation()
    return Measurement(name, time.perf_counter() - started, parsed, visible)


def make_corpus(root: Path, count: int) -> None:
    for index in range(count):
        folder = root / f"topic-{index % 32:02d}" / f"part-{index % 8:02d}"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"note-{index:05d}.md").write_text(
            f"---\ntitle: Note {index}\ntags: [topic-{index % 32}]\n---\n\n"
            f"# Note {index}\n\nA stable body for benchmark document {index}.\n",
            encoding="utf-8",
        )


def source_files(root: Path) -> list[Path]:
    return sorted(root.rglob("*.md"))


def parse(path: Path) -> dict[str, object]:
    text = path.read_text(encoding="utf-8")
    title = next((line[7:] for line in text.splitlines() if line.startswith("title: ")), path.stem)
    return {
        "title": title,
        "bytes": len(text.encode()),
        "sha256": hashlib.sha256(text.encode()).hexdigest(),
    }


def memory_start(root: Path) -> tuple[int, int]:
    entries = [parse(path) for path in source_files(root)]
    return len(entries), len(entries)


def files_cache_start(root: Path, cache_path: Path) -> tuple[int, int]:
    previous = json.loads(cache_path.read_text(encoding="utf-8")) if cache_path.exists() else {}
    current: dict[str, dict[str, object]] = {}
    parsed = 0
    for path in source_files(root):
        key = str(path.relative_to(root))
        stat = path.stat()
        identity = {"mtime_ns": stat.st_mtime_ns, "size": stat.st_size}
        cached = previous.get(key)
        if cached and all(cached.get(field) == value for field, value in identity.items()):
            current[key] = cached
        else:
            current[key] = {**identity, **parse(path)}
            parsed += 1
    cache_path.write_text(json.dumps(current, sort_keys=True), encoding="utf-8")
    return parsed, len(current)


def sqlite_initialize(db_path: Path, root: Path) -> None:
    with sqlite3.connect(db_path) as db:
        db.executescript(
            """
            CREATE TABLE IF NOT EXISTS notes (
                note_id TEXT PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                sha256 TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS branches (
                branch_id TEXT PRIMARY KEY,
                note_id TEXT NOT NULL,
                parent_note_id TEXT NOT NULL,
                position INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS branches_parent_position
                ON branches(parent_note_id, position);
            """
        )
        for path in source_files(root):
            content = path.read_text(encoding="utf-8")
            relative = str(path.relative_to(root))
            note_id = hashlib.sha256(relative.encode()).hexdigest()[:16]
            title = next(
                (line[7:] for line in content.splitlines() if line.startswith("title: ")), path.stem
            )
            digest = hashlib.sha256(content.encode()).hexdigest()
            db.execute(
                "INSERT OR IGNORE INTO notes(note_id,path,title,content,sha256) VALUES(?,?,?,?,?)",
                (note_id, relative, title, content, digest),
            )
        db.commit()


def sqlite_restart(db_path: Path) -> tuple[int, int]:
    with sqlite3.connect(db_path) as db:
        visible = db.execute("SELECT count(*) FROM notes").fetchone()[0]
    return 0, int(visible)


def sqlite_update(db_path: Path, root: Path, target: Path) -> tuple[int, int]:
    content = target.read_text(encoding="utf-8")
    relative = str(target.relative_to(root))
    digest = hashlib.sha256(content.encode()).hexdigest()
    with sqlite3.connect(db_path) as db:
        db.execute(
            "UPDATE notes SET content = ?, sha256 = ? WHERE path = ?", (content, digest, relative)
        )
        visible = db.execute("SELECT count(*) FROM notes").fetchone()[0]
    return 1, int(visible)


def print_measurement(measurement: Measurement) -> None:
    print(
        f"{measurement.name:28} {measurement.seconds * 1000:9.2f} ms "
        f"parsed={measurement.parsed:5} visible={measurement.visible:5}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--files", type=int, default=11_000)
    parser.add_argument(
        "--root",
        type=Path,
        help="measure an existing Markdown vault without modifying it",
    )
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="miku-vault-model-") as directory:
        root = args.root.resolve() if args.root else Path(directory) / "vault"
        if args.root:
            files = source_files(root)
            if not files:
                raise SystemExit(f"no Markdown files found under {root}")
            file_count = len(files)
        else:
            root.mkdir()
            make_corpus(root, args.files)
            file_count = args.files
            files = source_files(root)
        target = files[file_count // 2]

        print(f"corpus={file_count} files root={root}")
        print("\n[filesystem + memory projection]")
        print_measurement(timed("cold startup", lambda: memory_start(root)))
        print_measurement(timed("restart", lambda: memory_start(root)))

        cache_path = Path(directory) / "cache.json"
        print("\n[filesystem + durable manifest cache]")
        print_measurement(timed("cold startup", lambda: files_cache_start(root, cache_path)))
        print_measurement(timed("restart", lambda: files_cache_start(root, cache_path)))
        target.write_text(target.read_text(encoding="utf-8") + "\nchanged\n", encoding="utf-8")
        print_measurement(timed("one external edit", lambda: files_cache_start(root, cache_path)))

        db_path = Path(directory) / "trilium-model.sqlite"
        print("\n[SQLite notes + branches graph]")
        print_measurement(
            timed(
                "cold import", lambda: sqlite_initialize(db_path, root) or (file_count, file_count)
            )
        )
        print_measurement(timed("restart", lambda: sqlite_restart(db_path)))
        print_measurement(timed("one app edit", lambda: sqlite_update(db_path, root, target)))

        print("\ninterpretation:")
        print("- memory is simplest and fastest in-process, but restart is always a cold parse")
        print("- files-cache preserves Markdown authority and makes restart metadata-only")
        print("- SQLite graph makes restart cheap, but content authority moves into the database")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
