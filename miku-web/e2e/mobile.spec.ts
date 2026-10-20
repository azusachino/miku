// Phone-width layout: navigation drawer, tabs, and page width, migrated from
// scripts/ux_browser.py.
import { type Page } from "@playwright/test";
import { expect, test } from "./fixtures";

test.use({ viewport: { width: 390, height: 844 } });

const NOTES = [
  { path: "usage.md", title: "Using Miku Note" },
  { path: "changelog.md", title: "Changelog" },
  { path: "sandbox.md", title: "Markdown Sandbox" }
];

function drawerToggle(page: Page) {
  return page.getByRole("banner").getByRole("button", { name: /workspace navigation$/ });
}

function drawer(page: Page) {
  return page.getByRole("complementary", { name: "Workspace navigation" });
}

// Opens Home, then each note in NOTES through the drawer, leaving four tabs.
async function openNotesThroughDrawer(page: Page) {
  await page.goto("/p/index.md");
  for (const note of NOTES) {
    await drawerToggle(page).click();
    await expect(drawerToggle(page)).toHaveAttribute("aria-expanded", "true");
    await drawer(page).getByRole("button", { name: note.title, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`/p/${note.path.replace(".", "\\.")}$`));
    await expect(drawerToggle(page)).toHaveAttribute("aria-expanded", "false");
  }
}

test("the drawer starts closed and off screen", async ({ page }) => {
  await page.goto("/p/index.md");

  await expect(drawerToggle(page)).toHaveAccessibleName("Open workspace navigation");
  await expect(drawerToggle(page)).toHaveAttribute("aria-expanded", "false");
  await expect(drawer(page)).not.toBeInViewport();
});

test("the drawer opens, navigates, and closes after each navigation", async ({ page }) => {
  await openNotesThroughDrawer(page);

  await expect(page.getByRole("main").getByRole("heading", { name: "Markdown Sandbox", level: 1 })).toBeVisible();
});

test("Escape closes the drawer and returns focus to its toggle", async ({ page }) => {
  await page.goto("/p/index.md");
  await drawerToggle(page).click();
  await expect(drawer(page)).toBeInViewport();

  await page.keyboard.press("Escape");

  await expect(drawerToggle(page)).toHaveAttribute("aria-expanded", "false");
  await expect(drawerToggle(page)).toBeFocused();
});

test("closing the active tab shows the previous one without reopening it", async ({ page }) => {
  await openNotesThroughDrawer(page);
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(4);

  await page.getByRole("button", { name: "Close Markdown Sandbox" }).click();

  await expect(page).toHaveURL(/\/p\/changelog\.md$/);
  await expect(tabs).toHaveCount(3);
  await expect(page.getByRole("tab", { name: /Markdown Sandbox/ })).toHaveCount(0);
});

test("many open tabs scroll sideways instead of widening the page", async ({ page }, testInfo) => {
  await openNotesThroughDrawer(page);
  const lastTab = page.getByRole("tab").last();

  // Four tabs overflow a phone-width tab strip; the last one scrolls into view.
  await expect(lastTab).not.toBeInViewport({ ratio: 1 });
  await lastTab.scrollIntoViewIfNeeded();
  await expect(lastTab).toBeInViewport({ ratio: 1 });

  const pageOverflows = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
  expect(pageOverflows, "the page scrolls horizontally").toBe(false);
  await testInfo.attach("narrow.png", { body: await page.screenshot({ fullPage: true }), contentType: "image/png" });
});
