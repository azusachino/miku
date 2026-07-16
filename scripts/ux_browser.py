#!/usr/bin/env python3
"""Browser acceptance checks for the current Miku React shell."""

from __future__ import annotations

import os
from pathlib import Path

from playwright.sync_api import Page, sync_playwright

BASE_URL = os.environ.get("MIKU_UX_BROWSER_URL", "http://127.0.0.1:5173").rstrip("/")
NOTE_TITLE = os.environ.get("MIKU_UX_BROWSER_NOTE_TITLE", "Android JVM TI机制详解（内含福利彩蛋）")
ARTIFACT_DIR = Path(os.environ.get("MIKU_UX_ARTIFACT_DIR", ".artifacts/ux"))


def open_nested_note(page: Page) -> None:
    for folder in ("geektime-docs", "前端-移动", "Android开发高手课"):
        page.locator(".tree-row").filter(has_text=folder).first.click()
        page.wait_for_timeout(200)
    docs = page.locator(".tree-row").filter(has_text="docs")
    docs.nth(docs.count() - 1).click()
    page.wait_for_timeout(300)
    target = page.locator(".tree-row").filter(has_text=NOTE_TITLE).first
    if target.count() != 1:
        raise AssertionError(f"nested note is not uniquely visible: {NOTE_TITLE}")
    target.click()
    page.wait_for_timeout(700)


def main() -> int:
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, timeout=10_000)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.set_default_timeout(10_000)
        console_errors: list[str] = []
        page.on("pageerror", lambda error: console_errors.append(str(error)))
        page.goto(f"{BASE_URL}/", wait_until="domcontentloaded")
        page.get_by_text("All notes").wait_for()
        rows = page.locator(".tree-row")
        rows.first.wait_for(timeout=120_000)
        page.wait_for_timeout(700)
        if rows.count() == 0:
            raise AssertionError("workspace tree has no clickable rows")
        if page.locator(".tree-row").filter(has_text="adr").count() != 1:
            raise AssertionError("migrated ADR folder is missing from the vault tree")
        if page.locator(".tree-row").filter(has_text="architecture").count() != 1:
            raise AssertionError("migrated architecture note is missing from the vault tree")
        page.locator(".tree-row").filter(has_text="Sandbox").click()
        page.wait_for_url("**/p/Sandbox.md")
        page.locator(".note-scroll h1").filter(has_text="Sandbox").wait_for()
        if page.locator(".note-scroll h1").first.inner_text() != "Sandbox":
            raise AssertionError("clicking a note did not update the reader")
        if page.locator(".note-meta-tags .tag", has_text="#demo").count() != 1:
            raise AssertionError("sandbox inline tag is missing from note metadata")
        if page.locator(".context-title", has_text="Tags").count() != 0:
            raise AssertionError("tags are duplicated in the Context panel")
        backlink_text = "\n".join(page.locator(".backlink-row").all_inner_texts())
        for source in ("Changelog", "Features", "Index", "Usage"):
            if source not in backlink_text:
                raise AssertionError(f"Sandbox is missing its self-doc backlink from {source}")
        if "Sandbox.md" in backlink_text:
            raise AssertionError("a note must not list itself as a backlink")
        if (
            page.locator(".markdown-alert-note").count() != 1
            or page.locator(".markdown-alert-warning").count() != 1
        ):
            raise AssertionError("GitHub alert classes were not preserved in the browser")
        page.locator(".mermaid-diagram svg").wait_for(timeout=10_000)
        if page.locator(".mermaid-diagram svg").count() != 1:
            raise AssertionError("sandbox Mermaid diagram did not render")
        if page.locator(".katex").count() < 1:
            raise AssertionError("sandbox math did not render")
        if page.locator('a[href="/tags/demo"]').count() < 1:
            raise AssertionError("sandbox inline tag link is missing")
        page.locator(".note-meta-tags .tag", has_text="#demo").click()
        page.wait_for_url("**/tags/demo")
        page.get_by_role("button", name="Sandbox", exact=True).first.wait_for()
        page.goto(f"{BASE_URL}/p/Sandbox.md", wait_until="domcontentloaded")
        page.locator(".toc-item").first.click()
        if "#" not in page.url:
            raise AssertionError("TOC click did not update the URL fragment")

        open_nested_note(page)
        if "/p/geektime-docs/" not in page.url:
            raise AssertionError(f"nested note did not use the /p route: {page.url}")
        if page.locator(".note-scroll h1").first.inner_text() != NOTE_TITLE:
            raise AssertionError("nested note title did not render")
        if len(page.locator(".note-scroll").inner_text()) < 100:
            raise AssertionError("nested note rendered without content")
        if page.get_by_text("Loading note…").count():
            raise AssertionError("reader remained in the loading state")

        nested_folder = page.locator(".tree-row").filter(has_text="geektime-docs").first
        if nested_folder.get_attribute("aria-expanded") != "true":
            raise AssertionError("active note did not open its ancestor folders")
        nested_folder.click()
        page.wait_for_timeout(200)
        if page.locator(".tree-row").filter(has_text=NOTE_TITLE).count() != 0:
            raise AssertionError("closing a folder left its descendants visible")
        nested_folder.click()
        page.locator(".tree-row").filter(has_text=NOTE_TITLE).first.wait_for()
        if page.locator(".tree-row[aria-current='page']").filter(has_text=NOTE_TITLE).count() != 1:
            raise AssertionError("reopening a folder did not preserve the active note")
        page.get_by_role("button", name="Collapse workspace tree").click()
        if page.locator(".tree-row").filter(has_text=NOTE_TITLE).count() != 0:
            raise AssertionError("collapsed workspace tree still shows descendants")
        page.get_by_role("button", name="Expand workspace tree").click()
        page.locator(".tree-row").filter(has_text=NOTE_TITLE).first.wait_for()

        theme = page.locator(".app-shell").get_attribute("data-theme")
        dark_background = page.locator(".app-shell").evaluate(
            "el => getComputedStyle(el).backgroundColor"
        )
        page.get_by_role("button", name="Toggle theme").click()
        if page.locator(".app-shell").get_attribute("data-theme") == theme:
            raise AssertionError("theme toggle did not change the shell theme")
        light_background = page.locator(".app-shell").evaluate(
            "el => getComputedStyle(el).backgroundColor"
        )
        if dark_background == light_background:
            raise AssertionError("theme toggle changed the attribute but not the shell colors")

        page.get_by_role("button", name="Open quick search").click()
        quick_search = page.get_by_label("Quick search input")
        quick_search.wait_for()
        quick_search.fill("Features")
        if quick_search.input_value() != "Features":
            raise AssertionError("quick search panel input did not accept text")
        page.get_by_role("group", name="Search scope").wait_for()
        content_scope = page.get_by_role("button", name="Content")
        content_scope.click()
        if content_scope.get_attribute("aria-pressed") != "true":
            raise AssertionError("content search scope was not selectable")
        page.get_by_role("button", name="Title").click()
        if page.get_by_role("button", name="Title").get_attribute("aria-pressed") != "true":
            raise AssertionError("title search scope was not selectable")
        quick_search.press("ArrowDown")
        quick_search.press("Enter")
        page.wait_for_url("**/p/Features.md")

        page.set_viewport_size({"width": 390, "height": 844})
        for path in ("Index.md", "Usage.md", "Changelog.md", "Sandbox.md"):
            title = path.removesuffix(".md")
            page.locator(".tree-row").filter(has_text=title).last.click()
            page.wait_for_url(f"**/p/{path}")
            page.wait_for_timeout(250)
        tabs = page.locator(".tabs")
        if tabs.evaluate("el => getComputedStyle(el).overflowX") not in ("auto", "scroll"):
            raise AssertionError("open tabs are not horizontally scrollable")
        if not tabs.evaluate("el => el.scrollWidth > el.clientWidth"):
            raise AssertionError("multiple open tabs did not overflow horizontally")

        page.goto(f"{BASE_URL}/p/does-not-exist.md", wait_until="domcontentloaded")
        page.get_by_role("heading", name="Note unavailable", exact=True).wait_for()
        page.goto(f"{BASE_URL}/tags/not-a-real-tag", wait_until="domcontentloaded")
        page.get_by_role("heading", name="#not-a-real-tag").wait_for()
        if page.locator(".tag-note-row").count() != 0:
            raise AssertionError("missing tag unexpectedly returned notes")

        if page.get_by_role("button", name="Open vault menu").count():
            raise AssertionError("removed fake vault switcher is still exposed")
        page.goto(f"{BASE_URL}/", wait_until="domcontentloaded")
        page.get_by_role("button", name="Recent").click()
        page.wait_for_url("**/recent")
        page.get_by_role("heading", name="Recent notes").wait_for()
        page.goto(f"{BASE_URL}/", wait_until="domcontentloaded")
        page.get_by_role("button", name="Settings").click()
        page.get_by_role("dialog", name="Settings").wait_for()

        page.goto(f"{BASE_URL}/", wait_until="domcontentloaded")
        page.screenshot(path=str(ARTIFACT_DIR / "reading.png"), full_page=True)
        page.set_viewport_size({"width": 390, "height": 844})
        page.reload(wait_until="domcontentloaded")
        page.get_by_text("All notes").wait_for()
        if page.locator("body").evaluate("el => el.scrollWidth > el.clientWidth"):
            raise AssertionError("narrow viewport has horizontal overflow")
        page.screenshot(path=str(ARTIFACT_DIR / "narrow.png"), full_page=True)
        browser.close()

    if console_errors:
        raise AssertionError(f"browser page errors: {console_errors}")
    print(f"UX browser acceptance passed; artifacts={ARTIFACT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
