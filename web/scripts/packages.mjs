// What the app's npm packages are held to (ADR-0002, and the owner's choice
// of 2026-11-17 for packages): one list for what the app ships, a longer one
// for development tools, which never ship. `check-licences.mjs` and
// `check-advisories.mjs` are the commands; these are their rules.
import { existsSync, readFileSync } from "node:fs";

/**
 * Licences for what the app ships: `deny.toml`'s list for crates, with 0BSD
 * and MIT-0, which ask less than BSD and MIT do.
 */
export const SHIPPED = new Set([
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unicode-3.0",
  "CC0-1.0",
  "Unlicense",
  "0BSD",
  "MIT-0",
]);

/**
 * Licences for development tools: those, with Python-2.0, BlueOak-1.0.0,
 * CC-BY's data files, and MPL-2.0, which ADR-0002 allows unmodified.
 */
export const TOOLS = new Set([
  ...SHIPPED,
  "Python-2.0",
  "BlueOak-1.0.0",
  "CC-BY-4.0",
  "CC-BY-3.0",
  "MPL-2.0",
]);

/**
 * Whether an SPDX expression is allowed: one of an `OR`'s sides, both of an
 * `AND`'s. A `WITH` exception only loosens, so it is passed over. What isn't
 * an SPDX expression isn't allowed.
 * @param {string} expression
 * @param {Set<string>} allowed
 */
export function allows(expression, allowed) {
  const tokens = expression
    .replaceAll("(", " ( ")
    .replaceAll(")", " ) ")
    .split(/\s+/)
    .filter(Boolean);
  let at = 0;
  /** @returns {boolean} */
  const atom = () => {
    const token = tokens[at++];
    if (token === undefined) return false;
    if (token === "(") {
      const inner = or();
      return tokens[at++] === ")" && inner;
    }
    if (tokens[at] === "WITH") at += 2;
    return allowed.has(token);
  };
  /** @returns {boolean} */
  const and = () => {
    let all = atom();
    while (tokens[at] === "AND") {
      at++;
      all = atom() && all;
    }
    return all;
  };
  /** @returns {boolean} */
  const or = () => {
    let any = and();
    while (tokens[at] === "OR") {
      at++;
      any = and() || any;
    }
    return any;
  };
  const allowedHere = or();
  return at === tokens.length && allowedHere;
}

/**
 * @typedef {{ name: string, version: string, folder: string, licence: string }} Installed
 */

/**
 * Every package in the lock file, for every platform, with its licence: the
 * lock's own word, or else the installed manifest's.
 * @param {{ packages: Record<string, { version?: string, license?: unknown, link?: boolean }> }} lock
 * @returns {Installed[]}
 */
export function installed(lock) {
  return Object.entries(lock.packages)
    .filter(([folder, entry]) => folder !== "" && !entry.link)
    .map(([folder, entry]) => ({
      name: folder.slice(folder.lastIndexOf("node_modules/") + "node_modules/".length),
      version: entry.version ?? "?",
      folder,
      licence: licenceText(entry.license) ?? manifestLicence(folder) ?? "UNKNOWN",
    }));
}

/**
 * A manifest's licence as an SPDX expression: its `license`, or an old
 * manifest's list of them, one or another.
 * @param {unknown} licence
 * @returns {string | undefined}
 */
export function licenceText(licence) {
  if (typeof licence === "string") return licence;
  if (Array.isArray(licence))
    return licence.map((one) => licenceText(one) ?? "UNKNOWN").join(" OR ");
  if (licence && typeof licence === "object" && "type" in licence) return String(licence.type);
  return undefined;
}

/**
 * @param {string} folder
 * @returns {string | undefined}
 */
function manifestLicence(folder) {
  const manifest = `${folder}/package.json`;
  if (!existsSync(manifest)) return undefined;
  const { license, licenses } = JSON.parse(readFileSync(manifest, "utf8"));
  return licenceText(license ?? licenses);
}

/**
 * The packages whose licence their list refuses: the bundle's against
 * `SHIPPED`, the rest against `TOOLS`.
 * @param {Installed[]} packages
 * @param {Set<string>} bundled the names of the packages whose code is in the bundle
 */
export function refused(packages, bundled) {
  return packages
    .map((one) => ({ ...one, shipped: bundled.has(one.name) }))
    .filter((one) => !allows(one.licence, one.shipped ? SHIPPED : TOOLS));
}

/**
 * @typedef {{ id: string, package: string, severity?: string, title?: string }} Advisory
 * @typedef {{ id: string, package: string, reason: string, since: string }} Known
 */

/**
 * `npm audit --json`'s advisories, judged:
 *
 * - `shipped`: in a package the app ships, and refused;
 * - `fresh`: in a development tool, and not yet looked at, so refused too;
 * - `known`: in a development tool, and listed with its reason;
 * - `gone`: listed, and no longer reported: to be taken off the list.
 *
 * @param {{ vulnerabilities?: Record<string, { via: Array<string | { name: string, url: string, severity?: string, title?: string }> }> }} audit
 * @param {Known[]} known
 * @param {Set<string>} bundled
 */
export function judge(audit, known, bundled) {
  /** @type {Map<string, Advisory>} */
  const found = new Map();
  for (const entry of Object.values(audit.vulnerabilities ?? {})) {
    for (const via of entry.via) {
      if (typeof via === "string") continue;
      const id = via.url.slice(via.url.lastIndexOf("/") + 1);
      found.set(`${id} ${via.name}`, {
        id,
        package: via.name,
        ...(via.severity === undefined ? {} : { severity: via.severity }),
        ...(via.title === undefined ? {} : { title: via.title }),
      });
    }
  }
  const listed = new Set(known.map((one) => `${one.id} ${one.package}`));
  const advisories = [...found.values()];
  const isListed = (/** @type {Advisory} */ one) => listed.has(`${one.id} ${one.package}`);
  return {
    shipped: advisories.filter((one) => bundled.has(one.package)),
    fresh: advisories.filter((one) => !bundled.has(one.package) && !isListed(one)),
    known: advisories.filter((one) => !bundled.has(one.package) && isListed(one)),
    gone: known.filter((one) => !found.has(`${one.id} ${one.package}`)),
  };
}

/** The bundle's packages, which `npm run build` writes: none without a build. */
export function bundledPackages() {
  const file = ".bundle/packages.json";
  if (!existsSync(file)) return undefined;
  return new Set(/** @type {string[]} */ (JSON.parse(readFileSync(file, "utf8"))));
}
