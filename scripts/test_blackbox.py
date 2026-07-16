from pathlib import Path

from blackbox import validate_ready


def test_react_frontend_uses_path_routes_and_stable_note_selection() -> None:
    source = Path("miku-web/src/app.tsx").read_text(encoding="utf-8")
    assert 'path="/p/*"' in source
    assert "onSelect(node.path)" in source
    assert "useMemo(" in source


def test_frontend_keeps_editing_opt_in() -> None:
    source = Path("miku-web/src/app.tsx").read_text(encoding="utf-8")
    assert "MarkdownEditor" in source
    assert "editing" in source
    assert "setEditing(false)" in source


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
