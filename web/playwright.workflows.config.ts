import { defineConfig } from "@playwright/test";

import base from "./playwright.config.ts";

/**
 * Workflows end to end (W10.1): Milestone 2's Checkpoints G to L as suites,
 * each driving the app through its checkpoint's demo on a server of its own
 * from a clean data root, on the simulator's default samples and over real
 * distances. As the end-to-end tests, Chromium runs them all, and Firefox and
 * WebKit those of the main path; each waits for the instrument's own
 * conditions, never for a time. `npm run test:workflows`, after
 * `npm run build` and `cargo build -p oscope-server`.
 */
export default defineConfig({
  ...base,
  testDir: "tests/workflows",
  snapshotPathTemplate: "{testDir}/../e2e/baselines/{arg}-{projectName}-{platform}{ext}",
  // A checkpoint is a whole session at the instrument; a step stuck ends it in a minute.
  timeout: 600_000,
  use: { ...base.use, actionTimeout: 60_000 },
});
