// Headless Chromium verification, cloned from the playwright-verify skill.
// Miku needs two servers: the Rust API, run from the fixture directory so its
// `miku_docs/` vault is the small fixture under e2e/fixture, and the Vite dev
// server, which proxies /api to it. When a project script has already started
// the app, it exports E2E_BASE_URL and this config starts no server of its own.
import { defineConfig, devices } from "@playwright/test";

const PORT = 5107;
const API_PORT = 3107;
const API_URL = `http://127.0.0.1:${API_PORT}`;
const external = process.env.E2E_BASE_URL;
const CI = !!process.env.CI;
// Optional: a Chromium already on disk, for machines where this Playwright
// version's own build is not installed.
const executablePath = process.env.E2E_CHROMIUM_PATH || undefined;
const chrome = { ...devices["Desktop Chrome"], launchOptions: { executablePath } };

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  // Playwright's recommended CI settings: no stray test.only, retries that
  // expose flaky tests instead of hiding them, one worker.
  forbidOnly: CI,
  retries: CI ? 2 : 0,
  failOnFlakyTests: CI,
  workers: CI ? 1 : undefined,
  // A text reporter, because agents read the terminal; the html report needs a browser.
  reporter: "list",
  use: {
    baseURL: external ?? `http://127.0.0.1:${PORT}`,
    // Local runs do not retry, so keep a trace of every failure rather than of the first retry.
    trace: "retain-on-failure",
    screenshot: "only-on-failure"
  },
  // `make e2e` runs the checks; `make e2e-probe` runs only the probe.
  projects: [
    { name: "chromium", use: chrome, testIgnore: /probe\.spec\.ts$/ },
    { name: "probe", use: chrome, testMatch: /probe\.spec\.ts$/ }
  ],
  // No reuse: a port already in use fails the run instead of testing a server someone else is using.
  webServer: external
    ? undefined
    : [
        {
          // Read-only with the in-memory index, so a run never writes to the fixture.
          command: "cargo run --quiet -p miku",
          cwd: "./e2e/fixture",
          url: `${API_URL}/healthz`,
          env: { MIKU_BIND: `127.0.0.1:${API_PORT}`, MIKU_INDEX_BACKEND: "memory", MIKU_READONLY: "1" },
          reuseExistingServer: false,
          // The first run compiles the server.
          timeout: 300_000
        },
        {
          command: `bun run dev --port ${PORT} --strictPort`,
          url: `http://127.0.0.1:${PORT}`,
          env: { MIKU_API_URL: API_URL },
          reuseExistingServer: false,
          timeout: 60_000
        }
      ]
});
