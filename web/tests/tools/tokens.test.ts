import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { pairs } from "../../src/ui/tokens/pairs.ts";
import { accents } from "../../src/ui/tokens/theme.ts";
import { colour, contrast, root, rules, schemeOf, type Rgb, type Scheme } from "./css.ts";

const src = fileURLToPath(new URL("../../src/", import.meta.url));
const tokens = join(src, "ui", "tokens", "tokens.css");
const sheet = rules(readFileSync(tokens, "utf8"));
const schemes: Scheme[] = ["light", "dark"];

describe("the reader of the tokens", () => {
  const properties = new Map([
    ["--ink", "light-dark(#000000, #fff)"],
    ["--paper", "light-dark(white, var(--ink))"],
    ["--half", "color-mix(in srgb, var(--ink), white)"],
    ["--glass", "rgb(0 0 0 / 0.4)"],
  ]);
  const read = (value: string, scheme: Scheme = "light"): Rgb => colour(value, properties, scheme);

  it("works a value out as a browser does", () => {
    expect(read("#fff")).toEqual([1, 1, 1]);
    expect(read("#336699")).toEqual([0x33 / 255, 0x66 / 255, 0x99 / 255]);
    expect(read("var(--ink)", "light")).toEqual([0, 0, 0]);
    expect(read("var(--ink)", "dark")).toEqual([1, 1, 1]);
    // A token made of tokens, the scheme the same all the way down.
    expect(read("var(--paper)", "dark")).toEqual([1, 1, 1]);
    expect(read("light-dark(var(--paper), black)", "light")).toEqual([1, 1, 1]);
  });

  it("mixes two colours by their shares, as CSS Color 5 says", () => {
    expect(read("color-mix(in srgb, black 25%, white)")).toEqual([0.75, 0.75, 0.75]);
    expect(read("color-mix(in srgb, black, white 25%)")).toEqual([0.25, 0.25, 0.25]);
    expect(read("color-mix(in srgb, 25% black, white)")).toEqual([0.75, 0.75, 0.75]);
    expect(read("var(--half)")).toEqual([0.5, 0.5, 0.5]);
    // Shares over 100 % in all are scaled down to it.
    expect(read("color-mix(in srgb, black 150%, white 50%)")).toEqual([0.25, 0.25, 0.25]);
  });

  it("refuses what it can't measure", () => {
    expect(() => read("var(--glass)")).toThrow("not an opaque colour");
    expect(() => read("#ffffff80")).toThrow("not an opaque colour");
    expect(() => read("var(--missing)")).toThrow("no such token: --missing");
    expect(() => read("color-mix(in oklch, black, white)")).toThrow("only color-mix(in srgb");
    expect(() => read("color-mix(in srgb, black 20%, white 20%)")).toThrow("leave alpha");
    expect(() => read("light-dark(black)")).toThrow("two colours");
  });

  it("measures contrast as WCAG 2.2 does", () => {
    const white = read("#ffffff");
    expect(contrast(white, read("#000000"))).toBeCloseTo(21, 10);
    expect(contrast(white, white)).toBe(1);
    // The grey that just passes AA on white, and the one that just doesn't.
    expect(contrast(read("#767676"), white)).toBeCloseTo(4.54, 2);
    expect(contrast(white, read("#777777"))).toBeCloseTo(4.48, 2);
  });

  it("gives a root the rules that match it, the more specific and the later winning", () => {
    const css = `
      /* a comment { with braces } */
      :root, :root[data-accent="a"] { --x: 1; --y: 1; color-scheme: light dark; }
      :root[data-theme="dark"] { color-scheme: dark; }
      :root[data-accent="b"] { --x: 2; }
      :root { --y: 3; }
      .other[data-accent="b"] { --x: 9; }
      @media (prefers-reduced-motion: reduce) { :root { --x: 0; } }
    `;
    const parsed = rules(css);
    expect(Object.fromEntries(root(parsed, {}))).toEqual({
      "--x": "1",
      "--y": "3",
      "color-scheme": "light dark",
    });
    expect(root(parsed, { "data-accent": "b" }).get("--x")).toBe("2");
    expect(root(parsed, { "data-accent": "a" }).get("--x")).toBe("1");
    expect(root(parsed, {}, ["(prefers-reduced-motion: reduce)"]).get("--x")).toBe("0");
    expect(schemeOf(root(parsed, {}), "dark")).toBe("dark");
    expect(schemeOf(root(parsed, { "data-theme": "dark" }), "light")).toBe("dark");
  });
});

describe("the tokens", () => {
  describe.each(schemes)("in %s", (scheme) => {
    it.each(accents)("every allowed pair reads, with the accent %s", (accent) => {
      const properties = root(sheet, { "data-theme": scheme, "data-accent": accent });
      expect(schemeOf(properties, scheme === "light" ? "dark" : "light")).toBe(scheme);
      const short: string[] = [];
      for (const { ink, on, least } of pairs) {
        const ratio = contrast(
          colour(`var(--${ink})`, properties, scheme),
          colour(`var(--${on})`, properties, scheme),
        );
        if (ratio < least) short.push(`${ink} on ${on}: ${ratio.toFixed(2)}, under ${least}`);
      }
      expect(short).toEqual([]);
    });
  });

  it("follow the computer's scheme when no theme is chosen, with magenta", () => {
    const system = root(sheet, {});
    expect(schemeOf(system, "light")).toBe("light");
    expect(schemeOf(system, "dark")).toBe("dark");
    const magenta = root(sheet, { "data-accent": "magenta" });
    for (const token of ["--accent", "--accent-on", "--accent-text"]) {
      expect(system.get(token), token).toBe(magenta.get(token));
    }
  });

  it("style the accents the app offers, and no other", () => {
    const styled = sheet.flatMap((rule) =>
      rule.selectors.map((selector) => /data-accent="([^"]+)"/.exec(selector)?.[1]),
    );
    expect(styled.filter((accent) => accent !== undefined).sort()).toEqual([...accents].sort());
  });

  it("have no control under 24 px, which WCAG asks of a target", () => {
    const sizes = [...root(sheet, {})].filter(([token]) => /^--size-(control|rail)/.test(token));
    expect(sizes.length).toBeGreaterThanOrEqual(3);
    for (const [token, value] of sizes) {
      expect(value, token).toMatch(/^[\d.]+rem$/);
      expect(parseFloat(value) * 16, token).toBeGreaterThanOrEqual(24);
    }
  });

  it("have no motion for a reader who asks for less", () => {
    const usual = root(sheet, {});
    const reduced = root(sheet, {}, ["(prefers-reduced-motion: reduce)"]);
    for (const token of ["--motion-fast", "--motion-slow"]) {
      expect(usual.get(token), token).not.toBe("0ms");
      expect(reduced.get(token), token).toBe("0ms");
    }
  });

  it("are the only place a colour is written", () => {
    const written =
      /#(?:[0-9a-f]{3}){1,2}(?:[0-9a-f]{2})?\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/i;
    const files = readdirSync(src, { recursive: true, encoding: "utf8" })
      .map((file) => join(src, file))
      .filter(
        (file) => /\.(vue|css|ts)$/.test(file) && !file.endsWith(".test.ts") && file !== tokens,
      );
    expect(files.length).toBeGreaterThan(5);
    const found = files.flatMap((file) =>
      readFileSync(file, "utf8")
        .split("\n")
        .flatMap((line, at) =>
          written.test(line) ? [`${file.slice(src.length)}:${at + 1}: ${line.trim()}`] : [],
        ),
    );
    expect(found).toEqual([]);
  });
});
