// Smoke check cloned from the playwright-verify skill: the first route renders
// its main landmark without console errors.
import { expect, test } from "@playwright/test";

test("home renders its note without console errors", async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/");

  const main = page.getByRole("main");
  await expect(main).toBeVisible();
  await expect(main.getByRole("article").getByRole("heading", { name: "Home", level: 1 })).toBeVisible();
  expect(errors).toEqual([]);
  await testInfo.attach("reading.png", { body: await page.screenshot({ fullPage: true }), contentType: "image/png" });
});
