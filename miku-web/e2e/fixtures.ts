// Shared test fixture: every browser test fails on an uncaught page error,
// as scripts/ux_browser.py did for its whole run.
import { expect, test as base } from "@playwright/test";

export const test = base.extend<{ failOnPageErrors: void }>({
  failOnPageErrors: [
    async ({ page }, use) => {
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await use();
      expect(errors, "uncaught page errors").toEqual([]);
    },
    { auto: true }
  ]
});

export { expect };
