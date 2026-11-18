// Checks npm's advisories against the packages installed, as cargo-deny does
// for crates. An advisory in what the app ships fails. One in a development
// tool fails until it is listed in advisories.json, with its reason. Run it
// after a build, from web/:
//
//   npm run advisories
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

import { bundledPackages, judge } from "./packages.mjs";

const bundled = bundledPackages();
if (!bundled) {
  console.error("advisories: no .bundle/packages.json: run `npm run build` first");
  process.exit(2);
}
// npm exits with 1 when it finds any: its JSON says what.
const audit = spawnSync("npm", ["audit", "--json"], { encoding: "utf8", shell: true });
let report;
try {
  report = JSON.parse(audit.stdout);
} catch {
  console.error(`advisories: npm audit gave no report: ${audit.stderr || audit.stdout}`);
  process.exit(2);
}
if (report.error) {
  console.error(`advisories: npm audit failed: ${report.error.summary ?? report.error.code}`);
  process.exit(2);
}
const known = JSON.parse(readFileSync("advisories.json", "utf8"));
const { shipped, fresh, known: listed, gone } = judge(report, known, bundled);
for (const one of shipped) {
  console.error(`advisories: ${one.id} in ${one.package}, which the app ships: ${one.title}`);
}
for (const one of fresh) {
  console.error(
    `advisories: ${one.id} in ${one.package} (${one.severity}), a development tool: ${one.title}. ` +
      "Look at it, then list it in advisories.json with its reason",
  );
}
for (const one of gone) {
  console.log(
    `advisories: ${one.id} in ${one.package} is no longer reported: take it off advisories.json`,
  );
}
console.log(
  `advisories: ${shipped.length} in what the app ships, ${fresh.length} new and ` +
    `${listed.length} listed in development tools`,
);
process.exit(shipped.length || fresh.length ? 1 : 0);
