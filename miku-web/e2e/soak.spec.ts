// Timed navigation and search soak against the Rust API, migrated from
// scripts/ux_soak.py. It runs only in the `soak` project (`make e2e-soak`),
// not in `make e2e`. MIKU_UX_SOAK_SECONDS sets the duration (default 60) and
// MIKU_UX_SOAK_MAX_P95_SECONDS the latency budget for the last tenth of
// requests (default 5).
import { expect, test } from "@playwright/test";
import { API_URL } from "./servers";

const DURATION_MS = Number(process.env.MIKU_UX_SOAK_SECONDS ?? "60") * 1000;
const MAX_P95_MS = Number(process.env.MIKU_UX_SOAK_MAX_P95_SECONDS ?? "5") * 1000;
const PAGES = ["index.md", "changelog.md", "features.md", "usage.md"];
const SEARCHES = [
  { scope: "all", q: "Miku" },
  { scope: "title", q: "Home" },
  { scope: "content", q: "中文" }
];

test.use({ baseURL: API_URL });

function p95(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)];
}

test("navigation and search stay healthy under sustained load", async ({ request }) => {
  test.setTimeout(DURATION_MS + 60_000);
  const samples: { status: number; ms: number }[] = [];
  const timed = async (path: string, params?: Record<string, string>) => {
    const started = performance.now();
    const response = await request.get(path, { params, failOnStatusCode: false });
    samples.push({ status: response.status(), ms: performance.now() - started });
  };

  const deadline = Date.now() + DURATION_MS;
  for (let round = 0; Date.now() < deadline; round += 1) {
    await Promise.all([
      timed(`/api/v1/notes/${encodeURIComponent(PAGES[round % PAGES.length])}`),
      timed("/api/v1/search", SEARCHES[round % SEARCHES.length]),
      timed("/api/v1/tree", { folder: "library" }),
      timed("/api/v1/tags")
    ]);
  }

  const failures = samples.filter((sample) => sample.status < 200 || sample.status >= 400);
  const lastWindow = samples.slice(-Math.max(1, Math.floor(samples.length / 10))).map((sample) => sample.ms);
  const summary = { requests: samples.length, failures: failures.length, lastP95Ms: Math.round(p95(lastWindow)) };
  console.log(JSON.stringify(summary));

  expect(failures, "failed HTTP statuses").toEqual([]);
  expect(summary.lastP95Ms, "last-window p95 latency (ms)").toBeLessThanOrEqual(MAX_P95_MS);
});
