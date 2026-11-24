// What a change reaches (scripts/changed.mjs, ADR-0061): the Rust packages
// and the packages that depend on them, the examples and doc tests among
// them, the unit tests related to the app's files, and the end-to-end specs
// covering what changed, and the checkpoints' workflows for the app or the
// server; everything when the build, the checks, shared code, the harness or
// the server changed.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { coversOf, lines, packagesOf, reach } from "../../scripts/changed.mjs";

/**
 * A stand-in workspace: core, used by sim and by api; api by the server; an
 * example of sim's. The dependents come first, as cargo metadata may list
 * them: what reaches them has to be followed through, not read in order.
 */
const packages = [
  {
    name: "oscope-server",
    folder: "crates/server",
    dependencies: ["api"],
    example: false,
    library: true,
  },
  {
    name: "ex-sim",
    folder: "examples/ex-sim",
    dependencies: ["sim"],
    example: true,
    library: false,
  },
  { name: "api", folder: "crates/api", dependencies: ["core"], example: false, library: true },
  { name: "sim", folder: "crates/sim", dependencies: ["core"], example: false, library: true },
  { name: "core", folder: "crates/core", dependencies: [], example: false, library: true },
  { name: "lone", folder: "crates/lone", dependencies: [], example: false, library: true },
];

/** Stand-in specs: jobs's, the shell's over every feature, one without a `Covers:` line. */
const specs = [
  {
    file: "web/tests/e2e/jobs.spec.ts",
    covers: ["web/src/features/jobs"],
    text: "toHaveScreenshot(`jobs-${scheme}.png`)",
  },
  { file: "web/tests/e2e/shell.spec.ts", covers: ["web/src/features"], text: "" },
  { file: "web/tests/e2e/old.spec.ts", covers: undefined, text: "" },
  { file: "web/tests/e2e/power.spec.ts", covers: ["web/src/features/power"], text: "" },
];

const planOf = (...changed: string[]) => reach(changed, packages, specs);

describe("what a change reaches", () => {
  it("is everything when the build or the checks change", () => {
    for (const file of [
      "Cargo.lock",
      "Cargo.toml",
      "scripts/ci-local.sh",
      "web/scripts/changed.mjs",
    ]) {
      const plan = planOf(file);
      expect([plan.rust, plan.doc, plan.examples, plan.unit, plan.e2e], file).toEqual([
        "all",
        "all",
        "all",
        "all",
        "all",
      ]);
      expect(plan.python).toBe(true);
      expect(plan.workflows).toBe(true);
      expect(plan.why[0]).toContain(file);
    }
  });

  it("is nothing but the checks for docs", () => {
    const plan = planOf("docs/milestone-2/phases.md", "README.md");
    expect([plan.rust, plan.doc, plan.examples, plan.unit, plan.e2e]).toEqual([
      "none",
      "none",
      "none",
      "none",
      "none",
    ]);
    expect(plan.python).toBe(false);
    expect(plan.workflows).toBe(false);
  });

  it("is a crate's own tests and those of every package that depends on it", () => {
    const plan = planOf("crates/core/src/lib.rs");
    expect(plan.rust).toEqual(["oscope-server", "ex-sim", "api", "sim", "core"]);
    expect(plan.doc).toEqual(["oscope-server", "api", "sim", "core"]);
    expect(plan.examples).toEqual(["ex-sim"]);
    // The server is among them: every spec runs against it.
    expect(plan.e2e).toBe("all");
    expect(plan.why.join()).toContain("oscope-server");
  });

  it("leaves the specs alone for a crate the server doesn't use", () => {
    const plan = planOf("crates/sim/src/camera.rs", "crates/lone/README.md");
    expect(plan.rust).toEqual(["ex-sim", "sim", "lone"]);
    expect(plan.examples).toEqual(["ex-sim"]);
    expect(plan.e2e).toBe("none");
    expect(plan.unit).toBe("none");
  });

  it("is a feature's specs, the shell's and those without a line, and the related unit tests", () => {
    const plan = planOf(
      "web/src/features/jobs/JobDetail.vue",
      "web/src/features/jobs/queue.test.ts",
    );
    expect(plan.rust).toBe("none");
    expect(plan.unit).toEqual([
      "web/src/features/jobs/JobDetail.vue",
      "web/src/features/jobs/queue.test.ts",
    ]);
    expect(plan.e2e).toEqual([
      "web/tests/e2e/jobs.spec.ts",
      "web/tests/e2e/old.spec.ts",
      "web/tests/e2e/shell.spec.ts",
    ]);
  });

  it("is every spec for the app's shared code, the harness or the build of the app", () => {
    for (const file of [
      "web/src/ui/Button.vue",
      "web/src/api/client.ts",
      "web/src/app/router.ts",
      "web/tests/e2e/fixtures.ts",
      "web/tests/e2e/hide-frame.css",
      "web/playwright.config.ts",
      "web/package-lock.json",
    ]) {
      const plan = planOf(file);
      expect(plan.e2e, file).toBe("all");
      expect(plan.why.join(), file).toContain(file);
    }
  });

  it("picks no spec for the app's strings alone: the features' own see them", () => {
    const plan = planOf("web/src/app/locales/en.ts");
    expect(plan.e2e).toBe("none");
    expect(plan.unit).toEqual(["web/src/app/locales/en.ts"]);
  });

  it("is a spec changed, and the spec whose baseline changed", () => {
    expect(planOf("web/tests/e2e/power.spec.ts").e2e).toEqual(["web/tests/e2e/power.spec.ts"]);
    const plan = planOf("web/tests/e2e/baselines/jobs-dark-chromium-win32.png");
    expect(plan.e2e).toEqual(["web/tests/e2e/jobs.spec.ts"]);
    expect(plan.unit).toBe("none");
  });

  it("is every spec for a baseline no spec names", () => {
    const plan = planOf("web/tests/e2e/baselines/gone-light-chromium-linux.png");
    expect(plan.e2e).toEqual(specs.map((s) => s.file).sort());
    expect(plan.why.join()).toContain("gone-light");
  });

  it("is every unit test when the app's tooling changes", () => {
    expect(planOf("web/vite.config.ts").unit).toBe("all");
    expect(planOf("web/tsconfig.node.json").unit).toBe("all");
  });

  it("leaves out unit-test sources that were deleted", () => {
    const plan = reach(
      ["web/src/features/jobs/gone.ts", "web/src/features/jobs/kept.ts"],
      packages,
      specs,
      (file) => !file.endsWith("gone.ts"),
    );
    expect(plan.unit).toEqual(["web/src/features/jobs/kept.ts"]);
  });

  it("runs the workflows for the app, the server, the harness or themselves; not for another crate", () => {
    for (const file of [
      "web/src/features/jobs/JobsTab.vue",
      "crates/core/src/lib.rs",
      "web/tests/e2e/sim.ts",
      "web/tests/workflows/steps.ts",
      "web/playwright.workflows.config.ts",
    ]) {
      expect(planOf(file).workflows, file).toBe(true);
    }
    expect(planOf("crates/sim/src/camera.rs").workflows).toBe(false);
    expect(planOf("web/tests/e2e/jobs.spec.ts").workflows).toBe(false);
  });

  it("runs the Python SDK's tests when its folder changes", () => {
    expect(planOf("python/oscope-ext/oscope_ext/node.py").python).toBe(true);
  });

  it("says each, as the shell reads it", () => {
    const said = lines(planOf("crates/sim/src/lib.rs", "web/src/features/power/PowerTab.vue"));
    expect(said.split("\n")).toEqual([
      "rust: ex-sim sim",
      "doc: sim",
      "examples: ex-sim",
      "python: no",
      "unit: web/src/features/power/PowerTab.vue",
      "e2e: web/tests/e2e/old.spec.ts web/tests/e2e/power.spec.ts web/tests/e2e/shell.spec.ts",
      "workflows: yes",
    ]);
  });
});

describe("its inputs", () => {
  it("reads a spec's Covers line", () => {
    expect(coversOf("// Covers: web/src/features/a, web/src/features/b/\n// More.")).toEqual([
      "web/src/features/a",
      "web/src/features/b",
    ]);
    expect(coversOf("// No line.\nimport x from 'y';")).toBeUndefined();
  });

  it("reads the workspace from cargo metadata: path dependencies only, examples and libraries", () => {
    const metadata = {
      workspace_root: "/repo",
      packages: [
        {
          name: "core",
          manifest_path: "/repo/crates/core/Cargo.toml",
          dependencies: [{ name: "serde" }],
          targets: [{ kind: ["lib"] }],
        },
        {
          name: "ex-a",
          manifest_path: "/repo/examples/ex-a/Cargo.toml",
          dependencies: [{ name: "core", path: "/repo/crates/core" }, { name: "serde" }],
          targets: [{ kind: ["bin"] }],
        },
      ],
    };
    expect(packagesOf(metadata)).toEqual([
      { name: "core", folder: "crates/core", dependencies: [], example: false, library: true },
      {
        name: "ex-a",
        folder: "examples/ex-a",
        dependencies: ["core"],
        example: true,
        library: false,
      },
    ]);
  });

  it("finds a Covers line in every end-to-end spec", () => {
    const folder = join(import.meta.dirname, "..", "e2e");
    const missing = readdirSync(folder)
      .filter((name) => name.endsWith(".spec.ts"))
      .filter((name) => coversOf(readFileSync(join(folder, name), "utf8")) === undefined);
    expect(missing).toEqual([]);
  });
});
