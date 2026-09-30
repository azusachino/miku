// Reader rendering and navigation on the fixture sandbox note, migrated from
// scripts/ux_browser.py.
import { expect, test } from "./fixtures";

test.beforeEach(async ({ page }) => {
  await page.goto("/p/sandbox.md");
  await expect(page.getByRole("main").getByRole("heading", { name: "Markdown Sandbox", level: 1 })).toBeVisible();
});

test("clicking a note in the tree opens it in the reader", async ({ page }) => {
  await page.getByRole("complementary", { name: "Workspace navigation" }).getByRole("button", { name: "Features" }).click();

  await expect(page).toHaveURL(/\/p\/features\.md$/);
  await expect(page.getByRole("article").getByRole("heading", { name: "Features", level: 1 })).toBeVisible();
});

test("frontmatter tags render once, in source order", async ({ page }) => {
  const main = page.getByRole("main");
  await expect(main).toMatchAriaSnapshot(`
    - term: tags
    - definition:
      - button "#miku"
      - button "#demo"
      - button "#markdown"
  `);
  // Tags live in the frontmatter panel only, not again in the context panel.
  await expect(main.getByRole("complementary").getByText("Tags", { exact: true })).toHaveCount(0);
});

test("backlinks list every linking note but never the note itself", async ({ page }) => {
  const context = page.getByRole("main").getByRole("complementary");
  for (const source of ["Changelog changelog.md", "Features features.md", "Home index.md", "Using Miku Note usage.md"]) {
    await expect(context.getByRole("button", { name: source }).first()).toBeVisible();
  }
  await expect(context.getByRole("button", { name: /sandbox\.md$/ })).toHaveCount(0);
});

test("alerts, Mermaid, math, and inline tags render", async ({ page }) => {
  const article = page.getByRole("article");
  // Alert syntax becomes titled alerts rather than literal `[!NOTE]` text.
  await expect(article.getByText("NOTE", { exact: true })).toBeVisible();
  await expect(article.getByText("WARNING", { exact: true })).toBeVisible();
  await expect(article.getByText("[!NOTE]")).toHaveCount(0);
  // The Mermaid diagram replaces its placeholder with the rendered labels.
  await expect(article.getByText("Markdown file")).toBeVisible({ timeout: 10_000 });
  await expect(article.getByText("Rendering diagram…")).toHaveCount(0);
  await expect(article.getByRole("math").first()).toBeVisible();
  await expect(article.getByRole("link", { name: "#demo" })).toHaveAttribute("href", "/tags/demo");
});

test("a frontmatter tag opens its tag page", async ({ page }) => {
  await page.getByRole("main").getByRole("button", { name: "#demo" }).click();

  await expect(page).toHaveURL(/\/tags\/demo$/);
  await expect(page.getByRole("heading", { name: "#demo", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Markdown Sandbox sandbox.md" })).toBeVisible();
});

test("a table of contents entry updates the URL fragment", async ({ page }) => {
  await page.getByRole("navigation", { name: "Table of contents" }).getByRole("link", { name: "Math" }).click();

  await expect(page).toHaveURL(/#math$/);
});
