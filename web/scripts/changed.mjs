// What a change reaches (ADR-0061): from the files changed since a base,
// the Rust packages whose tests run, the examples whose smoke runs run, and
// the app's unit tests and end-to-end specs. scripts/ci-local.sh --changed
// runs only those; everything else it checks runs in full.
//
//   node web/scripts/changed.mjs BASE [HEAD]
//
// HEAD is what changed: HEAD itself by default. It prints a line for each of
// `rust:`, `doc:` (the reached packages with a library), `examples:`,
// `python:`, `unit:` and `e2e:`, each `all`, `none`, or a list; then `why:`
// lines, saying what made one of them `all`.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * @typedef {{ name: string, folder: string, dependencies: string[], example: boolean, library: boolean }} Package
 * A workspace package: its folder from the root, the workspace packages it
 * depends on, any kind, whether it is an example with a binary, and whether
 * it has a library, with doc tests.
 * @typedef {{ file: string, covers: string[] | undefined, text: string }} Spec
 * An end-to-end spec: its path from the root, the folders its `Covers:`
 * line names, if it has one, and its text.
 * @typedef {"all" | "none" | string[]} Reach
 * @typedef {{ rust: Reach, doc: Reach, examples: Reach, python: boolean, unit: Reach, e2e: Reach, why: string[] }} Plan
 */

/** Files whose change reaches every check: the build's, and the checks' own. */
const EVERYTHING = [
  /^Cargo\.(toml|lock)$/,
  /^rust-toolchain(\.toml)?$/,
  /^\.cargo\//,
  /^\.github\/workflows\//,
  /^scripts\/(ci-local|ci-clone|pinned)\.sh$/,
  /^web\/scripts\/changed\.mjs$/,
];
/** Files whose change reaches every unit test. */
const ALL_UNIT = [
  /^web\/package(-lock)?\.json$/,
  /^web\/(vite|vitest)\.config\.ts$/,
  /^web\/tsconfig[^/]*\.json$/,
];
/** Files whose change reaches every end-to-end spec: the harness, and the build. */
const ALL_E2E = [
  /^web\/package(-lock)?\.json$/,
  /^web\/(vite|playwright)\.config\.ts$/,
  /^web\/index\.html$/,
  /^web\/tests\/e2e\/(?![^/]*\.spec\.ts$)(?!baselines\/)[^/]+$/,
];
/** The app's strings: each feature's own specs see a change to theirs. */
const STRINGS = /^web\/src\/app\/locales\//;
/** A baseline's file: its screenshot's name, before its scheme, browser and system. */
const BASELINE = /^web\/tests\/e2e\/baselines\/(.+)-(light|dark)-[a-z]+-[a-z0-9]+\.png$/;

/**
 * What a change to `changed` reaches.
 * @param {string[]} changed the files changed, from the repository's root
 * @param {Package[]} packages the workspace's packages
 * @param {Spec[]} specs the end-to-end specs
 * @param {(file: string) => boolean} exists whether a changed file is still there
 * @returns {Plan}
 */
export function reach(changed, packages, specs, exists = () => true) {
  /** @type {string[]} */
  const why = [];
  const everything = changed.filter((file) => EVERYTHING.some((rule) => rule.test(file)));
  if (everything.length > 0) {
    why.push(`everything: ${everything.join(", ")}`);
    return { rust: "all", doc: "all", examples: "all", python: true, unit: "all", e2e: "all", why };
  }

  // The packages holding a changed file, and every package that depends on
  // them, as a test or otherwise.
  const owner = (/** @type {string} */ file) =>
    packages
      .filter((p) => file.startsWith(`${p.folder}/`))
      .sort((a, b) => b.folder.length - a.folder.length)[0];
  const reached = new Set(changed.map(owner).flatMap((p) => (p ? [p.name] : [])));
  for (let grew = true; grew;) {
    grew = false;
    for (const p of packages) {
      if (!reached.has(p.name) && p.dependencies.some((d) => reached.has(d))) {
        reached.add(p.name);
        grew = true;
      }
    }
  }
  const rust = packages.filter((p) => reached.has(p.name)).map((p) => p.name);
  const examples = packages.filter((p) => p.example && reached.has(p.name)).map((p) => p.name);
  const doc = packages.filter((p) => p.library && reached.has(p.name)).map((p) => p.name);

  // The unit tests Vitest finds related to the app's changed files.
  const web = changed.filter((file) => file.startsWith("web/"));
  const allUnit = changed.filter((file) => ALL_UNIT.some((rule) => rule.test(file)));
  if (allUnit.length > 0) why.push(`every unit test: ${allUnit.join(", ")}`);
  const sources = web.filter((file) => /\.(ts|vue|mjs|json)$/.test(file) && exists(file));
  /** @type {Reach} */
  const unit = allUnit.length > 0 ? "all" : sources.length > 0 ? sources : "none";

  // The specs: every one when shared code, the harness or the server
  // changed; otherwise those changed, those whose baselines changed, and
  // those covering a changed feature's folder.
  const shared = web.filter(
    (file) =>
      file.startsWith("web/src/") && !file.startsWith("web/src/features/") && !STRINGS.test(file),
  );
  const harness = changed.filter((file) => ALL_E2E.some((rule) => rule.test(file)));
  const server = reached.has("oscope-server") ? ["the server, oscope-server"] : [];
  const broad = [...shared, ...harness, ...server];
  /** @type {Reach} */
  let e2e;
  if (broad.length > 0) {
    why.push(`every spec: ${broad.join(", ")}`);
    e2e = "all";
  } else {
    const chosen = new Set(
      changed.filter((file) => file.endsWith(".spec.ts") && file.startsWith("web/tests/e2e/")),
    );
    for (const file of changed) {
      const baseline = BASELINE.exec(file);
      if (baseline) {
        const name = baseline[1];
        const owners = specs.filter(
          (s) => s.text.includes(`${name}-\${`) || s.text.includes(`${name}-light`),
        );
        if (owners.length === 0) why.push(`every spec: no spec names the baseline ${file}`);
        for (const s of owners.length > 0 ? owners : specs) chosen.add(s.file);
      }
    }
    for (const s of specs) {
      const covers = s.covers ?? ["web/src"];
      if (
        web.some(
          (file) => covers.some((folder) => file.startsWith(`${folder}/`)) && !STRINGS.test(file),
        )
      ) {
        chosen.add(s.file);
      }
    }
    const existing = [...chosen].filter((file) => specs.some((s) => s.file === file)).sort();
    e2e = existing.length > 0 ? existing : "none";
  }
  return {
    rust: rust.length > 0 ? rust : "none",
    doc: doc.length > 0 ? doc : "none",
    examples: examples.length > 0 ? examples : "none",
    python: changed.some((file) => file.startsWith("python/")),
    unit,
    e2e,
    why,
  };
}

/**
 * A spec's `Covers:` line, in its opening comments: the folders, from the
 * repository's root, whose changes it is run for.
 * @param {string} text
 * @returns {string[] | undefined}
 */
export function coversOf(text) {
  const line = /^\/\/ Covers: (.+)$/m.exec(text);
  return line?.[1]
    ?.split(",")
    .map((folder) => folder.trim().replace(/\/$/, ""))
    .filter((folder) => folder.length > 0);
}

/**
 * The workspace's packages, from `cargo metadata`'s output.
 * @param {{ packages: { name: string, manifest_path: string, dependencies: { name: string, path?: string }[], targets: { kind: string[] }[] }[], workspace_root: string }} metadata
 * @returns {Package[]}
 */
export function packagesOf(metadata) {
  const names = new Set(metadata.packages.map((p) => p.name));
  return metadata.packages.map((p) => {
    const folder = relative(metadata.workspace_root, dirname(p.manifest_path)).replaceAll(
      "\\",
      "/",
    );
    return {
      name: p.name,
      folder,
      dependencies: p.dependencies
        .filter((d) => d.path !== undefined && names.has(d.name))
        .map((d) => d.name),
      example: folder.startsWith("examples/") && p.targets.some((t) => t.kind.includes("bin")),
      library: p.targets.some((t) => t.kind.includes("lib")),
    };
  });
}

/**
 * The plan's lines, as the shell reads them.
 * @param {Plan} plan
 * @returns {string}
 */
export function lines(plan) {
  const said = (/** @type {Reach} */ r) => (Array.isArray(r) ? r.join(" ") : r);
  return [
    `rust: ${said(plan.rust)}`,
    `doc: ${said(plan.doc)}`,
    `examples: ${said(plan.examples)}`,
    `python: ${plan.python ? "yes" : "no"}`,
    `unit: ${said(plan.unit)}`,
    `e2e: ${said(plan.e2e)}`,
    ...plan.why.map((reason) => `why: ${reason}`),
  ].join("\n");
}

/** The files changed from `base`'s merge base with `head` to `head`. */
function changedSince(
  /** @type {string} */ base,
  /** @type {string} */ head,
  /** @type {string} */ root,
) {
  const run = (/** @type {string[]} */ args) =>
    execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
  const from = run(["merge-base", base, head]);
  return run(["diff", "--name-only", from, head])
    .split("\n")
    .filter((file) => file.length > 0);
}

const main = fileURLToPath(import.meta.url) === process.argv[1];
if (main) {
  const base = process.argv[2];
  if (!base) {
    console.error("usage: node web/scripts/changed.mjs BASE [HEAD]");
    process.exit(2);
  }
  const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
  const changed = changedSince(base, process.argv[3] ?? "HEAD", root);
  const metadata = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps", "--locked"], {
      cwd: root,
      encoding: "utf8",
      maxBuffer: 64 << 20,
    }),
  );
  const folder = join(root, "web", "tests", "e2e");
  const specs = readdirSync(folder)
    .filter((name) => name.endsWith(".spec.ts"))
    .map((name) => {
      const text = readFileSync(join(folder, name), "utf8");
      return { file: `web/tests/e2e/${name}`, covers: coversOf(text), text };
    });
  const plan = reach(changed, packagesOf(metadata), specs, (file) => existsSync(join(root, file)));
  console.log(lines(plan));
}
