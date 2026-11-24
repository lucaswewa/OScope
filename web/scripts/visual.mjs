// The app's visual tests alone (W10.2): every baseline, in Chromium, with
// OSCOPE_VISUAL=1, which npm's scripts can't set the same way on Windows and
// Linux. The baselines are the owner's PC's (ADR-0041): elsewhere, their
// differences say little.
//
//   node scripts/visual.mjs [PLAYWRIGHT'S ARGUMENTS...]     (npm run test:visual)
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const playwright = join(import.meta.dirname, "..", "node_modules", "@playwright", "test", "cli.js");
const { status } = spawnSync(
  process.execPath,
  [playwright, "test", "--project", "chromium", "--grep", "@visual", ...process.argv.slice(2)],
  { stdio: "inherit", env: { ...process.env, OSCOPE_VISUAL: "1" } },
);
process.exit(status ?? 1);
