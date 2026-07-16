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
        page.locator(".tree-row").filter(has_text="Features").click()
        page.wait_for_url("**/p/Features.md")
        if page.locator(".note-scroll h1").first.inner_text() != "Features":
            raise AssertionError("clicking a note did not update the reader")

        open_nested_note(page)
        if "/p/geektime-docs/" not in page.url:
            raise AssertionError(f"nested note did not use the /p route: {page.url}")
        if page.locator(".note-scroll h1").first.inner_text() != NOTE_TITLE:
            raise AssertionError("nested note title did not render")
        if len(page.locator(".note-scroll").inner_text()) < 100:
            raise AssertionError("nested note rendered without content")
        if page.get_by_text("Loading note…").count():
            raise AssertionError("reader remained in the loading state")

        theme = page.locator(".app-shell").get_attribute("data-theme")
        page.get_by_role("button", name="Toggle theme").click()
        if page.locator(".app-shell").get_attribute("data-theme") == theme:
            raise AssertionError("theme toggle did not change the shell theme")

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
