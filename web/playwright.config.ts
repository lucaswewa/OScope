import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests (ADR-0040): the built app, served by a server of each
 * test's own (`tests/e2e/server.ts`). Chromium runs every test; Firefox and
 * WebKit run those tagged `@main`, the main path. Build the app and the
 * server first: `npm run build`, and `cargo build -p oscope-server`.
 */
export default defineConfig({
  testDir: "tests/e2e",
  fullyParallel: true,
  forbidOnly: true,
  reporter: "list",
  timeout: 60_000,
  // Each check waits for its condition, up to this long: never for a time.
  expect: { timeout: 15_000 },
  use: {
    viewport: { width: 1280, height: 800 },
    trace: "retain-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, grep: /@main/ },
    { name: "webkit", use: { ...devices["Desktop Safari"] }, grep: /@main/ },
  ],
});
