from blackbox import validate_health, validate_page


def test_validate_health_accepts_capabilities_payload() -> None:
    health = validate_health(
        "application/json",
        '{"status":"ok","capabilities":{"durable":true}}',
    )
    assert health["status"] == "ok"


def test_validate_health_rejects_non_json() -> None:
    try:
        validate_health("text/html", "ok")
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
