// HTTP smoke checks against the Rust API, migrated from scripts/ux_smoke.py.
// They call the API server directly, since the probes and metrics routes are
// not proxied by the Vite dev server.
import { expect, test } from "@playwright/test";
import { API_URL } from "./servers";

test.use({ baseURL: API_URL });

test("probes and metrics respond", async ({ request }) => {
  expect((await request.get("/healthz")).status()).toBe(200);
  expect([200, 503]).toContain((await request.get("/readyz")).status());
  expect((await request.get("/metrics")).status()).toBe(200);
  expect((await request.get("/api/openapi.json")).status()).toBe(200);
});

test("workspace, tree, and note APIs serve the vault", async ({ request }) => {
  const workspace = await request.get("/api/v1/workspace");
  expect(workspace.status()).toBe(200);
  expect(await workspace.json()).toMatchObject({ root: "miku_docs", note_count: expect.any(Number) });
  expect((await workspace.json()).note_count).toBeGreaterThan(0);

  const tree = await request.get("/api/v1/tree");
  expect(tree.status()).toBe(200);
  expect((await tree.json()).nodes.length).toBeGreaterThan(0);
  expect((await request.get("/api/v1/tree?folder=library")).status()).toBe(200);

  const note = await request.get("/api/v1/notes/index.md");
  expect(note.status()).toBe(200);
  expect(note.headers()["content-type"]).toContain("application/json");
  expect((await note.json()).title).toBe("Home");
});

test("every search scope finds vault content, including CJK text", async ({ request }) => {
  const queries = [
    { scope: "all", q: "Sandbox" },
    { scope: "title", q: "Features" },
    { scope: "content", q: "rendering" },
    { scope: "content", q: "中文" }
  ];
  for (const params of queries) {
    const response = await request.get("/api/v1/search", { params });
    expect(response.status(), `${params.scope}:${params.q}`).toBe(200);
    expect((await response.json()).results.length, `${params.scope}:${params.q}`).toBeGreaterThan(0);
  }
  expect((await request.get("/api/v1/tags")).status()).toBe(200);
});

test("concurrent note reads all succeed", async ({ request }) => {
  const paths = ["index.md", "changelog.md", "features.md", "usage.md"];
  const statuses = await Promise.all(paths.map(async (path) => (await request.get(`/api/v1/notes/${encodeURIComponent(path)}`)).status()));
  expect(statuses).toEqual(paths.map(() => 200));
});
