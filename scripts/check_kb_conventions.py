#!/usr/bin/env python3
"""Check conventions for Miku's maintained first-party knowledge base."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "miku_docs"
KEBAB_NAME = re.compile(r"^(?:\d{4}-)?[a-z0-9]+(?:-[a-z0-9]+)*\.md$")
MARKDOWN_LINK = re.compile(r"\[[^\]]+\]\(([^)]+\.md)(?:#[^)]*)?\)")
WIKILINK = re.compile(r"\[\[([^]|#]+)")


def maintained_notes() -> list[Path]:
    notes = list(DOCS.glob("*.md"))
    notes.extend((DOCS / "adr").glob("*.md"))
    return sorted(notes)


def main() -> int:
    notes = maintained_notes()
    relative_paths = {note.relative_to(DOCS).as_posix() for note in notes}
    errors: list[str] = []

    for note in notes:
        relative = note.relative_to(ROOT).as_posix()
        if not KEBAB_NAME.fullmatch(note.name):
            errors.append(f"{relative}: filename must be lowercase kebab-case")

        text = note.read_text(encoding="utf-8")
        for target in MARKDOWN_LINK.findall(text):
            if "://" in target:
                continue
            resolved = (note.parent / target).resolve()
            try:
                target_relative = resolved.relative_to(DOCS.resolve()).as_posix()
            except ValueError:
                continue
            if target_relative not in relative_paths:
                errors.append(f"{relative}: unresolved Markdown link {target!r}")

        for target in WIKILINK.findall(text):
            parent = note.parent.relative_to(DOCS).as_posix()
            candidate = f"{parent}/{target}.md".removeprefix("./")
            if candidate not in relative_paths:
                candidate = f"{target}.md"
            if candidate in relative_paths:
                continue
            matches = [path for path in relative_paths if path.casefold() == candidate.casefold()]
            if matches:
                errors.append(
                    f"{relative}: wikilink {target!r} must use canonical target {matches[0][:-3]!r}"
                )

    if errors:
        print("KB convention check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"KB convention check passed ({len(notes)} maintained notes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
