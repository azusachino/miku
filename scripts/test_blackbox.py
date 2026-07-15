from pathlib import Path

from blackbox import validate_page, validate_ready


def test_navigation_does_not_prefetch_full_pages_on_hover() -> None:
    source = Path("src/templates/base.html").read_text(encoding="utf-8")
    assert "prefetchPageLink" not in source
    assert "X-Miku-Prefetch" not in source


def test_file_tree_is_read_only_in_v0_0_2() -> None:
    shell = Path("static/miku.js").read_text(encoding="utf-8")
    template = Path("src/templates/base.html").read_text(encoding="utf-8")
    features = Path("miku_docs/Features.md").read_text(encoding="utf-8")
    usage = Path("miku_docs/Usage.md").read_text(encoding="utf-8")
    api = Path("docs/api_design_spec.md").read_text(encoding="utf-8")
    css = Path("static/miku.css").read_text(encoding="utf-8")
    routes = Path("src/http.rs").read_text(encoding="utf-8")
    assert "miku-open-rename" not in template
    assert "dragstart" not in template
    assert "draggable" not in template
    assert "/api/v1/move" not in shell
    assert "/api/v1/trash" not in shell
    assert "/api/v1/move" not in routes
    assert "/api/v1/trash" not in routes
    assert "miku_docs/.trash" not in features
    assert "miku_docs/.trash" not in usage
    assert "`/api/v1/trash`" not in api
    assert ".mk-rename-input" not in css


def test_validate_ready_accepts_capabilities_payload() -> None:
    health = validate_ready(
        "application/json",
        '{"status":"ok","capabilities":{"durable":true}}',
    )
    assert health["status"] == "ok"


def test_validate_ready_rejects_non_json() -> None:
    try:
        validate_ready("text/html", "ok")
    except AssertionError as error:
        assert "expected JSON" in str(error)
    else:
        raise AssertionError("non-JSON health response must fail")


def test_validate_page_accepts_miku_render() -> None:
    validate_page("<html><title>Miku</title></html>", "Index")


def test_validate_page_rejects_unrelated_html() -> None:
    try:
        validate_page("<html>other app</html>", "Index")
    except AssertionError as error:
        assert "does not look" in str(error)
    else:
        raise AssertionError("unrelated HTML must fail")
