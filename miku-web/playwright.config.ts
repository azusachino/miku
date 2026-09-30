// Headless Chromium verification, cloned from the playwright-verify skill.
// Miku needs two servers: the Rust API, run from the fixture directory so its
// `miku_docs/` vault is the small fixture under e2e/fixture, and the Vite dev
// server, which proxies /api to it. When a project script has already started
// the app, it exports E2E_BASE_URL and this config starts no server of its own.
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig, devices } from "@playwright/test";
import { API_PORT, API_URL, PORT } from "./e2e/servers";

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
  // `make e2e` runs the checks; `make e2e-probe` runs only the probe;
  // `make e2e-soak` runs only the timed API soak.
  projects: [
    { name: "chromium", use: chrome, testIgnore: /(probe|soak)\.spec\.ts$/ },
    { name: "probe", use: chrome, testMatch: /probe\.spec\.ts$/ },
    { name: "soak", testMatch: /soak\.spec\.ts$/ }
  ],
  // No reuse: a port already in use fails the run instead of testing a server someone else is using.
  webServer: external
    ? undefined
    : [
        {
          // The default SQLite index, rebuilt from the fixture in a fresh temporary
          // file each run; read-only, so a run never writes to the fixture vault.
          command: "cargo run --quiet -p miku",
          cwd: "./e2e/fixture",
          url: `${API_URL}/healthz`,
          env: {
            MIKU_BIND: `127.0.0.1:${API_PORT}`,
            MIKU_INDEX_BACKEND: "sqlite",
            MIKU_INDEX_PATH: join(tmpdir(), `miku-e2e-${process.pid}.sqlite`),
            MIKU_READONLY: "1"
          },
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
