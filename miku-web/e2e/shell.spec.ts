// Workspace shell behavior, migrated from scripts/ux_browser.py.
import { expect, test } from "./fixtures";

test("the theme toggle changes the shell colors and the favicon", async ({ page }) => {
  await page.goto("/p/index.md");
  const banner = page.getByRole("banner");
  await expect(banner).toBeVisible();
  // The favicon lives in <head>, which has no accessible roles, so this is the
  // one place the suite locates an element by CSS.
  const favicon = page.locator('link[rel="icon"]');
  await expect(favicon).toHaveAttribute("href", /miku-icon-(dark|light)\.svg$/);
  const iconBefore = await favicon.getAttribute("href");
  const backgroundBefore = await banner.evaluate((element) => getComputedStyle(element).backgroundColor);

  await page.getByRole("button", { name: "Toggle theme" }).click();

  await expect(favicon).not.toHaveAttribute("href", iconBefore ?? "");
  await expect(favicon).toHaveAttribute("href", /miku-icon-(dark|light)\.svg$/);
  await expect(banner).not.toHaveCSS("background-color", backgroundBefore);
});

test("quick search switches scope, opens a result, and starts empty next time", async ({ page }) => {
  await page.goto("/p/index.md");
  await page.getByRole("button", { name: "Open quick search" }).click();
  const input = page.getByLabel("Quick search input");
  await input.fill("Features");
  await expect(input).toHaveValue("Features");

  const scopes = page.getByRole("group", { name: "Search scope" });
  await scopes.getByRole("button", { name: "Content" }).click();
  await expect(scopes.getByRole("button", { name: "Content" })).toHaveAttribute("aria-pressed", "true");
  await scopes.getByRole("button", { name: "Title" }).click();
  await expect(scopes.getByRole("button", { name: "Title" })).toHaveAttribute("aria-pressed", "true");

  // Switching scope clears the previous results; wait for fresh ones before
  // driving the keyboard, as a user would.
  await expect(page.getByRole("option").first()).toBeVisible();
  await input.press("ArrowDown");
  await input.press("Enter");
  await expect(page).toHaveURL(/\/p\/features\.md$/);

  await page.getByRole("button", { name: "Open quick search" }).click();
  await expect(page.getByLabel("Quick search input")).toHaveValue("");
  await page.keyboard.press("Escape");
});

test("a missing note redirects home with a notice", async ({ page }) => {
  await page.goto("/p/does-not-exist.md");

  await expect(page).toHaveURL(/\/p\/index\.md$/);
  await expect(page.getByRole("alert").filter({ hasText: "Note not found" })).toBeVisible();
});

test("a tag with no notes shows an empty tag page", async ({ page }) => {
  await page.goto("/tags/not-a-real-tag");

  const main = page.getByRole("main");
  await expect(main.getByRole("heading", { name: "#not-a-real-tag", level: 1 })).toBeVisible();
  await expect(main.getByRole("button", { name: /\.md$/ })).toHaveCount(0);
});

test("Recent and Settings open from the sidebar, with no vault switcher", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Open vault menu" })).toHaveCount(0);

  await page.getByRole("button", { name: "Recent" }).click();
  await expect(page).toHaveURL(/\/recent$/);
  await expect(page.getByRole("heading", { name: "Recent notes" })).toBeVisible();

  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
});
