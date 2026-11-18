import { defineConfig, devices } from "@playwright/test";

// What a run writes, a failure's trace above all, goes in a folder of its
// own, named by when the run started and by its process. Playwright clears
// its folder as a run starts, so a shared one would lose the files of the
// runs beside it. Workers load this file again: the name reaches them in the
// environment.
const now = new Date();
const two = (part: number) => String(part).padStart(2, "0");
const day = `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}`;
const time = `${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
const run = (process.env.OSCOPE_E2E_RUN ??= `${day}-${time}-${process.pid}`);

/**
 * End-to-end tests (ADR-0040): the built app, served by a server of each
 * test's own (`tests/e2e/server.ts`). Chromium runs every test; Firefox and
 * WebKit run those tagged `@main`, the main path. Build the app and the
 * server first: `npm run build`, and `cargo build -p oscope-server`.
 */
export default defineConfig({
  testDir: "tests/e2e",
  outputDir: `test-results/${run}`,
  fullyParallel: true,
  forbidOnly: true,
  reporter: [["list"], ["./tests/e2e/tidy.ts"]],
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
