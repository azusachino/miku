"""Contract checks shared by reader-shell HTTP and browser acceptance tests."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

READER_PAGE_FIELDS = frozenset(
    {
        "path",
        "title",
        "exists",
        "html",
        "content_html",
        "toc",
        "backlinks",
        "unlinked_mentions",
        "word_count",
        "backlink_count",
        "updated",
        "frontmatter",
        "breadcrumbs",
    }
)


def validate_reader_page_payload(payload: Any) -> dict[str, Any]:
    """Validate and return the stable page-read JSON contract."""
    if not isinstance(payload, Mapping):
        raise AssertionError("reader page payload must be a JSON object")

    missing = sorted(READER_PAGE_FIELDS - payload.keys())
    if missing:
        raise AssertionError(f"reader page payload is missing fields: {', '.join(missing)}")

    if not isinstance(payload["path"], str) or not payload["path"]:
        raise AssertionError("reader page payload path must be a non-empty string")
    if not isinstance(payload["title"], str):
        raise AssertionError("reader page payload title must be a string")
    if not isinstance(payload["exists"], bool):
        raise AssertionError("reader page payload exists must be boolean")
    if not isinstance(payload["html"], str) or not payload["html"]:
        raise AssertionError("reader page payload html must be a non-empty string")
    if "<script" in payload["html"].lower() or "<link" in payload["html"].lower():
        raise AssertionError("reader page payload html must not contain scripts or stylesheets")
    if not isinstance(payload["content_html"], str):
        raise AssertionError("reader page payload content_html must be a string")
    for field in ("toc", "backlinks", "unlinked_mentions", "breadcrumbs"):
        if not isinstance(payload[field], list):
            raise AssertionError(f"reader page payload {field} must be an array")
    for field in ("word_count", "backlink_count"):
        if not isinstance(payload[field], int) or isinstance(payload[field], bool):
            raise AssertionError(f"reader page payload {field} must be an integer")
    if not isinstance(payload["frontmatter"], Mapping):
        raise AssertionError("reader page payload frontmatter must be an object")

    return dict(payload)
