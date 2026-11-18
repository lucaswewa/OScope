// Checks every package in the lock file against the licences OScope allows
// (ADR-0002): what the bundle holds against the list for what ships, the rest
// against the list for development tools. Run it after a build, from web/:
//
//   npm run licences [-- --list]
//
// --list also prints how many packages have each licence.
import { readFileSync } from "node:fs";

import { bundledPackages, installed, refused } from "./packages.mjs";

const bundled = bundledPackages();
if (!bundled) {
  console.error("licences: no .bundle/packages.json: run `npm run build` first");
  process.exit(2);
}
const packages = installed(JSON.parse(readFileSync("package-lock.json", "utf8")));
if (process.argv.includes("--list")) {
  for (const shipped of [true, false]) {
    /** @type {Map<string, number>} */
    const counts = new Map();
    for (const one of packages.filter((p) => bundled.has(p.name) === shipped)) {
      counts.set(one.licence, (counts.get(one.licence) ?? 0) + 1);
    }
    console.log(shipped ? "what the app ships:" : "development tools:");
    for (const [licence, count] of [...counts].sort((a, b) => b[1] - a[1])) {
      console.log(`  ${String(count).padStart(4)}  ${licence}`);
    }
  }
}
const bad = refused(packages, bundled);
for (const one of bad) {
  const where = one.shipped ? "what the app ships" : "a development tool";
  console.error(`licences: ${one.name}@${one.version} is ${one.licence}: not allowed for ${where}`);
}
const inBundle = packages.filter((p) => bundled.has(p.name)).length;
console.log(
  `licences: ${packages.length} packages, ${inBundle} in the bundle, ${bad.length} refused`,
);
process.exit(bad.length ? 1 : 0);
