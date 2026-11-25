import { defineConfig } from "@playwright/test";

import base from "./playwright.config.ts";

/**
 * The app's budgets (W10.3, ADR-0059): `tests/perf/`, in Chromium alone, one
 * test at a time, so that each measures the computer to itself. `cargo xtask
 * perf --web` runs them against the server it built in release, and reads
 * their `perf:` lines; `cargo xtask soak --web` keeps the live view open as
 * long as the soak, OSCOPE_PERF_MINUTES.
 */
export default defineConfig({
  ...base,
  testDir: "tests/perf",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // The memory's test lasts as long as it is asked to.
  timeout: 0,
  projects: [{ name: "chromium", use: { ...base.projects?.[0]?.use } }],
});
