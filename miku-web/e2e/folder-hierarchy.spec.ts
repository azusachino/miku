// The folder tree is the hierarchy (ADR-0024), checked against the fixture
// vault in e2e/fixture/miku_docs.
import { expect, test } from "./fixtures";

test("a folder expands to show its notes", async ({ page }) => {
  await page.goto("/p/index.md");
  const tree = page.getByRole("complementary", { name: "Workspace navigation" });

  await tree.getByRole("button", { name: "projects" }).click();

  await expect(tree.getByRole("button", { name: "Alpha" })).toBeVisible();
  await expect(tree.getByRole("button", { name: "Beta" })).toBeVisible();
});

test("legacy parents and order are shown as ordinary properties", async ({ page }) => {
  await page.goto("/p/projects/alpha.md");
  const main = page.getByRole("main");
  await expect(main.getByRole("article").getByRole("heading", { name: "Alpha", level: 1 })).toBeVisible();

  await expect(main).toMatchAriaSnapshot(`
    - term: order
    - definition: "3"
    - term: parents
    - definition: note-projects
  `);
  // The placement count row is gone with the placement model.
  await expect(main.getByText("placements")).toHaveCount(0);
});

test("a path-qualified wikilink resolves to the existing note", async ({ page }) => {
  await page.goto("/p/inbox.md");
  const main = page.getByRole("main");

  // Outgoing links show the resolved note's title, not a missing target.
  await expect(main.getByRole("button", { name: "Alpha projects/alpha.md" })).toBeVisible();

  await main.getByRole("article").getByRole("link", { name: "projects/alpha" }).click();
  await expect(page).toHaveURL(/\/p\/projects\/alpha\.md$/);
  await expect(main.getByRole("article").getByRole("heading", { name: "Alpha", level: 1 })).toBeVisible();
});
