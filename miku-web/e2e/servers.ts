// Where the e2e servers listen, shared by playwright.config.ts and the specs
// that call the Rust API directly. E2E_BASE_URL and E2E_API_URL point a run at
// servers a project script has already started.
export const PORT = 5107;
export const API_PORT = 3107;
export const API_URL = process.env.E2E_API_URL ?? `http://127.0.0.1:${API_PORT}`;
