// The packages' rules (scripts/packages.mjs): licences and advisories, each
// in two tiers, what the app ships and development tools.
import { describe, expect, it } from "vitest";

import { allows, installed, judge, refused, SHIPPED, TOOLS } from "../../scripts/packages.mjs";

describe("a licence expression", () => {
  it.each([
    ["MIT", true],
    ["0BSD", true],
    ["(MIT OR CC0-1.0)", true],
    ["(MIT OR GPL-3.0-only)", true],
    ["MIT AND Apache-2.0", true],
    ["(MIT AND GPL-3.0-only)", false],
    ["Apache-2.0 WITH LLVM-exception", true],
    ["(BSD-2-Clause OR (GPL-2.0-only AND MIT))", true],
    ["GPL-3.0-only", false],
    ["LGPL-3.0-or-later", false],
    ["MPL-2.0", false],
    ["BlueOak-1.0.0", false],
    ["SEE LICENSE IN LICENSE.txt", false],
    ["UNKNOWN", false],
    ["", false],
    ["(MIT", false],
  ])("%j is allowed in what ships: %j", (expression, allowed) => {
    expect(allows(expression, SHIPPED)).toBe(allowed);
  });

  it("is allowed more in a development tool, and never copyleft", () => {
    for (const licence of ["MPL-2.0", "BlueOak-1.0.0", "Python-2.0", "CC-BY-4.0"]) {
      expect(allows(licence, TOOLS), licence).toBe(true);
      expect(allows(licence, SHIPPED), licence).toBe(false);
    }
    for (const licence of ["GPL-3.0-only", "AGPL-3.0-only", "LGPL-2.1-only", "UNLICENSED"]) {
      expect(allows(licence, TOOLS), licence).toBe(false);
    }
  });
});

const lock = {
  packages: {
    "": { version: "0.1.1" },
    "node_modules/vue": { version: "3.5.44", license: "MIT" },
    "node_modules/argparse": { version: "2.0.1", license: "Python-2.0" },
    "node_modules/@scope/tool": { version: "1.0.0", license: "GPL-3.0-only" },
    "node_modules/a/node_modules/vue": { version: "2.7.0", license: "MIT" },
    "node_modules/linked": { link: true },
    "node_modules/old": { version: "0.1.0", license: [{ type: "MIT" }, { type: "X11" }] },
    "node_modules/silent": { version: "0.0.1" },
  },
};

describe("the installed packages", () => {
  it("are read from the lock file, with nested and scoped names", () => {
    const names = installed(lock).map((one) => `${one.name}@${one.version}: ${one.licence}`);
    expect(names).toEqual([
      "vue@3.5.44: MIT",
      "argparse@2.0.1: Python-2.0",
      "@scope/tool@1.0.0: GPL-3.0-only",
      "vue@2.7.0: MIT",
      "old@0.1.0: MIT OR X11",
      "silent@0.0.1: UNKNOWN",
    ]);
  });

  it("are refused by the list of their tier", () => {
    const packages = installed(lock);
    // Nothing shipped: Python-2.0 passes as a tool's; the GPL and the unknown don't.
    expect(refused(packages, new Set()).map((one) => one.name)).toEqual(["@scope/tool", "silent"]);
    // Shipped, argparse is held to the shorter list.
    const shipped = refused(packages, new Set(["vue", "argparse"]));
    expect(shipped.map((one) => [one.name, one.shipped])).toEqual([
      ["argparse", true],
      ["@scope/tool", false],
      ["silent", false],
    ]);
  });
});

describe("npm's advisories", () => {
  const audit = {
    vulnerabilities: {
      picomatch: {
        via: [
          {
            name: "picomatch",
            url: "https://github.com/advisories/GHSA-aaaa",
            severity: "high",
            title: "slow",
          },
        ],
      },
      vite: { via: ["picomatch"] },
      marked: {
        via: [
          {
            name: "marked",
            url: "https://github.com/advisories/GHSA-bbbb",
            severity: "moderate",
            title: "xss",
          },
        ],
      },
    },
  };
  const known = [
    { id: "GHSA-aaaa", package: "picomatch", reason: "our own globs", since: "2026-11-17" },
    { id: "GHSA-cccc", package: "glob", reason: "fixed since", since: "2026-11-17" },
  ];

  it("fail in what ships, and in a tool until they are listed", () => {
    const tools = judge(audit, known, new Set());
    expect(tools.shipped).toEqual([]);
    expect(tools.fresh.map((one) => one.id)).toEqual(["GHSA-bbbb"]);
    expect(tools.known.map((one) => one.id)).toEqual(["GHSA-aaaa"]);
    expect(tools.gone.map((one) => one.id)).toEqual(["GHSA-cccc"]);
  });

  it("can't be listed away once the package ships", () => {
    const shipping = judge(audit, known, new Set(["picomatch", "marked"]));
    expect(shipping.shipped.map((one) => one.id).sort()).toEqual(["GHSA-aaaa", "GHSA-bbbb"]);
    expect(shipping.known).toEqual([]);
  });

  it("are none when npm reports none", () => {
    expect(judge({}, [], new Set())).toEqual({ shipped: [], fresh: [], known: [], gone: [] });
  });
});
