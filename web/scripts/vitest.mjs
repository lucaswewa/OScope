// Starts Vitest from this folder's path as the disk spells it. On Windows, a
// terminal may hand a program its folder with a lower-case drive letter, as
// VS Code's do. Vite then names the same files under the upper-case one, so
// Vitest is loaded twice, and every test fails before it starts with "Cannot
// read properties of undefined (reading 'config')".
//
//   node scripts/vitest.mjs run     (npm run test:unit)
import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { join } from "node:path";

const root = realpathSync.native(process.cwd());
const vitest = join(root, "node_modules", "vitest", "vitest.mjs");
const { status } = spawnSync(process.execPath, [vitest, ...process.argv.slice(2)], {
  cwd: root,
  stdio: "inherit",
});
process.exit(status ?? 1);
