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
    page.goto(f"{BASE_URL}{PAGE_PATH}", wait_until="domcontentloaded", timeout=10_000)
    page.locator("link[rel='icon']").wait_for(state="attached")
    assert_visible(page, ".mk-topbar", "shell topbar")
    assert_visible(page, ".mk-sidebar", "shell sidebar")
    assert_visible(page, ".mk-article", "reading article")
    page.screenshot(path=str(ARTIFACT_DIR / "reading.png"), full_page=True)


def check_palette(page: Page) -> None:
    page.keyboard.press("Control+K")
    assert_visible(page, ".mk-command-modal", "quick switcher")
    palette = page.locator("input[x-ref='paletteInput']")
    palette.fill("Index")
    page.locator(".mk-command-item").first.wait_for()
    page.keyboard.press("ArrowDown")
    page.keyboard.press("Escape")
    if page.locator(".mk-command-modal").is_visible():
        raise AssertionError("quick switcher did not close with Escape")


def check_navigation(page: Page) -> None:
    page.locator("a[href='/p/Features']").first.click()
    page.wait_for_url("**/p/Features")
    if "Features" not in page.locator("h1").inner_text():
        raise AssertionError("page navigation did not render Features")
    page.go_back(wait_until="domcontentloaded")
    page.wait_for_url(f"**{PAGE_PATH}")


def main() -> int:
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, timeout=10_000)
        page = browser.new_page(reduced_motion="reduce", viewport={"width": 1280, "height": 900})
        page.set_default_timeout(5_000)
        page.set_default_navigation_timeout(10_000)
        try:
            check_shell(page)
            check_palette(page)
            check_navigation(page)
            page.set_viewport_size({"width": 390, "height": 844})
            page.reload(wait_until="domcontentloaded")
            page.screenshot(path=str(ARTIFACT_DIR / "narrow.png"), full_page=True)
            if page.locator("body").evaluate("el => el.scrollWidth > el.clientWidth"):
                raise AssertionError("narrow viewport has horizontal overflow")
        finally:
            browser.close()
    print(f"UX browser acceptance passed; artifacts={ARTIFACT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
