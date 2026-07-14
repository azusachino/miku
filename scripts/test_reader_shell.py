from reader_shell_contract import READER_PAGE_FIELDS, validate_reader_page_payload


def valid_payload() -> dict[str, object]:
    return {
        "path": "Index",
        "title": "Index",
        "exists": True,
        "content_html": "<p>Welcome</p>",
        "toc": [],
        "backlinks": [],
        "unlinked_mentions": [],
        "word_count": 1,
        "backlink_count": 0,
        "updated": "2026-07-15 08:00",
        "frontmatter": {},
        "breadcrumbs": [],
    }


def test_reader_page_contract_has_only_stable_read_fields() -> None:
    assert READER_PAGE_FIELDS == {
        "path",
        "title",
        "exists",
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


def test_reader_page_contract_accepts_valid_payload() -> None:
    assert validate_reader_page_payload(valid_payload())["path"] == "Index"


def test_reader_page_contract_rejects_missing_fields() -> None:
    payload = valid_payload()
    del payload["content_html"]

    try:
        validate_reader_page_payload(payload)
    except AssertionError as error:
        assert "content_html" in str(error)
    else:
        raise AssertionError("missing reader payload fields must fail validation")


def test_reader_page_contract_rejects_wrong_types() -> None:
    payload = valid_payload()
    payload["word_count"] = True

    try:
        validate_reader_page_payload(payload)
    except AssertionError as error:
        assert "word_count" in str(error)
    else:
        raise AssertionError("invalid reader payload types must fail validation")
