// Workspace tree behavior on a note three folders deep, migrated from
// scripts/ux_browser.py.
import { type Page } from "@playwright/test";
import { expect, test } from "./fixtures";

const DEEP_NOTE = "Deep Note";

function tree(page: Page) {
  return page.getByRole("complementary", { name: "Workspace navigation" });
}

async function openDeepNote(page: Page) {
  await page.goto("/p/index.md");
  for (const folder of ["library", "course", "docs"]) {
    await tree(page).getByRole("button", { name: folder, exact: true }).click();
  }
  await tree(page).getByRole("button", { name: DEEP_NOTE }).click();
}

test("a nested note opens through its folders on a /p route", async ({ page }) => {
  await openDeepNote(page);

  await expect(page).toHaveURL(/\/p\/library\/course\/docs\/deep-note\.md$/);
  const article = page.getByRole("article");
  await expect(article.getByRole("heading", { name: DEEP_NOTE, level: 1 })).toBeVisible();
  await expect(article.getByText(/three folders deep/)).toBeVisible();
  await expect(page.getByText("Loading note…")).toHaveCount(0);
});

test("the active note keeps its ancestors open, and folders hide and restore it", async ({ page }) => {
  await openDeepNote(page);
  const library = tree(page).getByRole("button", { name: "library", exact: true });
  const deepNote = tree(page).getByRole("button", { name: DEEP_NOTE });
  await expect(library).toHaveAttribute("aria-expanded", "true");

  await library.click();
  await expect(deepNote).toHaveCount(0);

  await library.click();
  await expect(deepNote).toBeVisible();
  await expect(deepNote).toHaveAttribute("aria-current", "page");
});

test("collapsing the whole tree hides descendants until a root folder is reopened", async ({ page }) => {
  await openDeepNote(page);
  const deepNote = tree(page).getByRole("button", { name: DEEP_NOTE });

  await tree(page).getByRole("button", { name: "Collapse workspace tree" }).click();
  await expect(deepNote).toHaveCount(0);

  await tree(page).getByRole("button", { name: "library", exact: true }).click();
  await expect(deepNote).toBeVisible();
  await expect(tree(page).getByRole("button", { name: "Collapse workspace tree" })).toHaveCount(1);
});
