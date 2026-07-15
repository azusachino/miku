#!/usr/bin/env python3
"""Deterministic browser acceptance checks for the Miku shell."""

from __future__ import annotations

import os
from pathlib import Path

from playwright.sync_api import Page, sync_playwright

BASE_URL = os.environ.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
PAGE_PATH = os.environ.get("MIKU_UX_BROWSER_PAGE", "/p/Index")
ARTIFACT_DIR = Path(os.environ.get("MIKU_UX_ARTIFACT_DIR", ".artifacts/ux"))


def assert_visible(page: Page, selector: str, label: str) -> None:
    if not page.locator(selector).is_visible():
        raise AssertionError(f"{label}: selector is not visible: {selector}")


def check_shell(page: Page) -> None:
    editor_imports: list[str] = []
    optional_reader_assets: list[str] = []
    event_stream_requests: list[str] = []

    def record_reader_import(request) -> None:
        if "esm.sh/" in request.url and request.resource_type == "script":
            editor_imports.append(request.url)
        if (
            "prism" in request.url.lower()
            or "mermaid" in request.url.lower()
            or "katex" in request.url.lower()
        ):
            optional_reader_assets.append(request.url)
        if request.url.endswith("/events"):
            event_stream_requests.append(request.url)

    page.on("request", record_reader_import)
    page.goto(f"{BASE_URL}{PAGE_PATH}", wait_until="domcontentloaded", timeout=10_000)
    page.wait_for_function("window.Alpine && document.body._x_dataStack", timeout=10_000)
    page.wait_for_timeout(250)
    page.remove_listener("request", record_reader_import)
    if editor_imports:
        raise AssertionError(
            "reader mode must not load external editor modules: " + ", ".join(editor_imports[:3])
        )
    if optional_reader_assets:
        raise AssertionError(
            "reader mode must not preload optional Prism/Mermaid assets: "
            + ", ".join(optional_reader_assets[:3])
        )
    if event_stream_requests:
        raise AssertionError("reader mode must not open the idle /events stream")
    page.locator("link[rel='icon']").wait_for(state="attached")
    assert_visible(page, ".mk-topbar", "shell topbar")
    assert_visible(page, ".mk-sidebar", "shell sidebar")
    assert_visible(page, ".mk-article", "reading article")
    if "miku_docs/" in page.locator("body").inner_text():
        raise AssertionError("reader chrome must not expose the filesystem root")
    layout = page.locator(".mk-article").evaluate(
        "article => {"
        "const main = article.closest('.mk-read').getBoundingClientRect();"
        "const rect = article.getBoundingClientRect();"
        "const rail = document.querySelector('.mk-rail');"
        "return {width: rect.width, center: rect.left + rect.width / 2, "
        "mainCenter: main.left + main.width / 2, "
        "rail: !!rail && getComputedStyle(rail).display !== 'none'};"
        "}"
    )
    if layout["width"] > 760:
        raise AssertionError(f"reading column is too wide: {layout['width']}")
    if abs(layout["center"] - layout["mainCenter"]) > 48:
        raise AssertionError("reading column is not centered in the main content area")
    if page.evaluate("window.innerWidth") <= 1280 and layout["rail"]:
        raise AssertionError("right rail must not squeeze the reading column at this width")
    assert_visible(page, ".mk-topbar-action:first-of-type", "topbar actions")
    if page.locator(".mk-history-controls").count():
        raise AssertionError("browser history arrows should not be duplicated in the app chrome")
    if page.locator(".mk-mobile-files").count():
        raise AssertionError("broken mobile files control must not be rendered")
    actions = page.locator(".mk-topbar-action")
    if actions.count() != 1:
        raise AssertionError("topbar must expose exactly one compact action control")
    for index in range(actions.count()):
        action = actions.nth(index)
        if not action.get_attribute("aria-label") or not action.get_attribute("title"):
            raise AssertionError(
                "compact topbar actions must retain accessible labels and tooltips"
            )
        if action.locator("span:visible").count():
            raise AssertionError("compact topbar actions must not show descriptive text")
    if page.locator(".mk-topbar .mk-icon:visible").count() < 3:
        raise AssertionError("topbar controls are missing their icons")
    if page.locator("button[aria-label='Open settings'] .mk-icon:visible").count() != 1:
        raise AssertionError("settings control must render exactly one visible icon")
    page.locator("[data-set-theme='light']").first.click()
    page.wait_for_function("document.documentElement.dataset.theme === 'light'")
    if not page.locator("[data-set-theme='light'].is-active").count():
        raise AssertionError("light theme control did not become selected")
    page.locator("[data-set-theme='dark']").first.click()
    page.wait_for_function("document.documentElement.dataset.theme === 'dark'")
    page.screenshot(path=str(ARTIFACT_DIR / "reading.png"), full_page=True)


def check_persistence(page: Page) -> None:
    state = page.evaluate("""() => {
        const keys = Object.keys(localStorage);
        return { keys, state: JSON.parse(localStorage.getItem('miku:ui:v1') || 'null') };
    }""")
    if state["state"] is None or state["state"].get("version") != 1:
        raise AssertionError("versioned UI state was not persisted")
    legacy = [key for key in state["keys"] if key.startswith("miku:") and key != "miku:ui:v1"]
    if legacy:
        raise AssertionError(f"legacy UI state keys remain: {legacy}")
    page.reload(wait_until="domcontentloaded")
    page.wait_for_function("document.documentElement.dataset.theme === 'dark'")
    assert_visible(page, ".mk-article", "article after direct refresh")
    assert_visible(page, ".mk-h1", "page title after direct refresh")


def check_palette(page: Page) -> None:
    page.keyboard.press("Control+K")
    page.locator(".mk-command-modal").wait_for(state="visible")
    palette = page.locator("input[x-ref='paletteInput']")
    palette.fill("Index")
    page.locator(".mk-command-item").first.wait_for()
    page.keyboard.press("ArrowDown")
    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
    if page.locator(".mk-command-modal").is_visible():
        raise AssertionError("quick switcher did not close with Escape")

    page.keyboard.press("Control+Shift+P")
    page.locator(".mk-command-modal").wait_for(state="visible")
    page.locator(".mk-command-tabs button").filter(has_text="Pages").click()
    page.locator(".mk-command-item").first.wait_for()
    if page.locator(".mk-command-item").filter(has_text="Toggle Zen mode").count():
        raise AssertionError("command palette retained command items in Pages mode")
    page.keyboard.press("Escape")


def check_tags(page: Page) -> None:
    page.get_by_role("tab", name="Tags").click()
    tag_index = page.locator("#sidebar-tags .mk-sidebar-link")
    tag_index.wait_for(state="visible")
    tag_index.click()
    page.wait_for_url("**/tags")
    if "Tags" not in page.locator("h1").first.inner_text():
        raise AssertionError("tag index did not render")
    if page.locator(".mk-tag-index [data-tag-cloud]").count():
        if page.locator("[data-tag-sentinel]").count():
            before = page.locator(".mk-tag-index [data-tag-cloud] > a").count()
            page.locator("[data-tag-sentinel]").scroll_into_view_if_needed()
            page.wait_for_function(
                "before => document.querySelectorAll("
                "'.mk-tag-index [data-tag-cloud] > a'"
                ").length > before",
                arg=before,
                timeout=15_000,
            )
        if page.locator("[data-tag-load-more]").count():
            raise AssertionError("tag index must use scroll loading, not a button")
    page.goto(f"{BASE_URL}/tags/release", wait_until="domcontentloaded")
    if "miku_docs/" in page.locator("body").inner_text():
        raise AssertionError("tag results must not expose the filesystem root")
    if page.locator("[data-tag-page-load-more]").count():
        raise AssertionError("tag results must use scroll loading, not a button")
    sentinel = page.locator("[data-tag-page-sentinel]")
    if sentinel.count():
        initial_results = page.locator("[data-tag-page-list] > a").count()
        sentinel.scroll_into_view_if_needed()
        page.wait_for_function(
            "initial => document.querySelectorAll('[data-tag-page-list] > a').length > initial || "
            "!document.querySelector('[data-tag-page-sentinel]')",
            arg=initial_results,
        )
    page.goto(f"{BASE_URL}{PAGE_PATH}", wait_until="domcontentloaded")


def check_zen_mode(page: Page) -> None:
    page.keyboard.press("Control+Shift+P")
    page.locator(".mk-command-modal").wait_for(state="visible")
    page.locator("input[x-ref='paletteInput']").fill("Toggle Zen mode")
    page.get_by_text("Toggle Zen mode", exact=True).click()
    page.locator("body.mk-zen").wait_for(state="attached")
    page.locator(".mk-zen-exit-button").wait_for(state="visible")
    page.keyboard.press("Escape")
    page.wait_for_timeout(200)
    if page.locator("body.mk-zen").count():
        raise AssertionError("Zen mode did not close with Escape")


def check_navigation(page: Page) -> None:
    tree_link = page.locator(".tree-link[href='/p/Features']").first
    if tree_link.get_attribute("data-reader-nav") != "true":
        raise AssertionError("tree page links must use persistent reader navigation")
    document_requests: list[str] = []

    def record_document(request) -> None:
        if request.resource_type == "document":
            document_requests.append(request.url)

    page.on("request", record_document)
    tree_link.click()
    page.wait_for_url("**/p/Features")
    page.wait_for_timeout(250)
    page.remove_listener("request", record_document)
    if document_requests:
        raise AssertionError(f"reader navigation reloaded the document: {document_requests}")
    if "Features" not in page.locator(".mk-h1").inner_text():
        raise AssertionError("page navigation did not render Features")
    page.locator(".mk-prose .mermaid svg").wait_for(state="attached", timeout=10_000)
    page.locator(".mk-prose pre code .token").first.wait_for(state="attached", timeout=10_000)
    if page.locator(".mk-prose .mk-copy-btn").count() != 5:
        raise AssertionError("code blocks must receive one copy button each")
    if page.locator(".mk-article[data-page-updated]").count() != 1:
        raise AssertionError("reader fragments must expose their freshness marker")
    changelog = page.locator(".tree-link[href='/p/Changelog']").first
    changelog.click()
    page.wait_for_url("**/p/Changelog")
    page.locator(".mk-prose .mermaid svg").wait_for(state="attached", timeout=10_000)
    sandbox = page.locator(".tree-link[href='/p/Sandbox']").first
    sandbox.click()
    page.wait_for_url("**/p/Sandbox")
    page.locator(".mk-prose .mermaid svg").wait_for(state="attached", timeout=10_000)
    page.locator(".mk-prose pre code .token").first.wait_for(state="attached", timeout=10_000)
    page.locator(".mk-prose .katex").nth(1).wait_for(state="attached", timeout=10_000)
    if page.locator(".mk-prose .katex").count() != 2:
        raise AssertionError("Sandbox math must render inline and display equations")
    if page.locator(".mk-prose .mk-copy-btn").count() != 1:
        raise AssertionError("Sandbox code block must receive one copy button")
    page.go_back()
    page.wait_for_url("**/p/Changelog")
    page.go_back()
    page.wait_for_url("**/p/Features")
    page.go_back()
    page.wait_for_url(f"**{PAGE_PATH}")


def check_inline_editor_is_lazy(page: Page) -> None:
    page.goto(f"{BASE_URL}{PAGE_PATH}", wait_until="domcontentloaded")
    imports: list[str] = []

    def record_editor_import(request) -> None:
        if "esm.sh/" in request.url and request.resource_type == "script":
            imports.append(request.url)

    page.on("request", record_editor_import)
    page.locator(".mk-title-actions button", has_text="Edit").click()
    page.locator("[data-inline-editor] .cm-editor").wait_for(state="attached", timeout=10_000)
    page.remove_listener("request", record_editor_import)
    if not imports:
        raise AssertionError("Edit did not load the CodeMirror editor modules")


def check_editor(page: Page) -> None:
    page.goto(f"{BASE_URL}{PAGE_PATH}", wait_until="domcontentloaded")
    page.goto(f"{BASE_URL}/p/Index/edit", wait_until="domcontentloaded")
    page.wait_for_url("**/p/Index/edit")
    assert_visible(page, "[data-editor]", "full editor")
    assert_visible(page, "[data-save-status]", "editor save status")
    form = page.locator("form[data-editor]")
    if form.get_attribute("hx-boost") != "false" or form.get_attribute("hx-history") != "false":
        raise AssertionError("editor save must use a normal POST redirect, not boosted history")
    page.locator("#editor-container .cm-editor").wait_for(state="attached", timeout=10_000)
    page.locator("#edit-title-input").fill("Index UX acceptance")
    if page.locator("[data-save-status]").inner_text() != "Unsaved changes":
        raise AssertionError("editor did not expose unsaved state")
    page.locator("a.mk-btn-ghost").first.click()
    page.wait_for_url("**/p/Index")


def main() -> int:
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, timeout=10_000)
        page = browser.new_page(reduced_motion="reduce", viewport={"width": 1280, "height": 900})
        page.set_default_timeout(5_000)
        page.set_default_navigation_timeout(10_000)
        console_errors: list[str] = []

        def record_console(message) -> None:
            if message.type == "error":
                console_errors.append(message.text)

        page.on("console", record_console)
        try:
            check_shell(page)
            check_persistence(page)
            check_palette(page)
            check_tags(page)
            check_zen_mode(page)
            check_navigation(page)
            check_inline_editor_is_lazy(page)
            check_editor(page)
            page.set_viewport_size({"width": 390, "height": 844})
            page.reload(wait_until="domcontentloaded")
            page.screenshot(path=str(ARTIFACT_DIR / "narrow.png"), full_page=True)
            if page.locator("body").evaluate("el => el.scrollWidth > el.clientWidth"):
                raise AssertionError("narrow viewport has horizontal overflow")
            history_errors = [error for error in console_errors if "historyCacheError" in error]
            if history_errors:
                raise AssertionError(f"HTMX history cache errors observed: {history_errors}")
        finally:
            browser.close()
    print(f"UX browser acceptance passed; artifacts={ARTIFACT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
