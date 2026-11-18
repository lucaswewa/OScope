import { describe, expect, it } from "vitest";

import { decimals, read, stepped, written } from "@/ui/number";

describe("what a number field reads", () => {
  it.each([
    ["12", 12],
    [" 12.5 ", 12.5],
    ["-0.25", -0.25],
    ["+3", 3],
    [".5", 0.5],
    ["5.", 5],
    ["1e3", 1000],
    ["2.5E-3", 0.0025],
    ["-0", 0],
    ["-0.00", 0],
  ])("takes %j as a number", (text, value) => {
    const taken = read(text, {});
    expect(taken).toEqual({ ok: true, value });
    // Never the -0 that equals 0 and is written differently.
    expect(taken.ok && Object.is(taken.value, value)).toBe(true);
  });

  it.each(["12,5", "1 000", "abc", "1.2.3", "--1", "0x10", "Infinity", "NaN", "1e", "e3", "12 µm"])(
    "refuses %j, which isn't one",
    (text) => {
      expect(read(text, {})).toEqual({ ok: false, why: "notANumber" });
    },
  );

  it("refuses a value outside the range, and says which side", () => {
    const range = { min: 0.1, max: 0.7 };
    expect(read("0.1", range)).toEqual({ ok: true, value: 0.1 });
    expect(read("0.7", range)).toEqual({ ok: true, value: 0.7 });
    expect(read("0.09", range)).toEqual({ ok: false, why: "least" });
    expect(read("0.71", range)).toEqual({ ok: false, why: "most" });
    // A range with one end has no other.
    expect(read("1e9", { min: 0 })).toEqual({ ok: true, value: 1e9 });
    expect(read("-1e9", { max: 0 })).toEqual({ ok: true, value: -1e9 });
  });

  it("takes nothing typed as no value, where the field is optional, and refuses it elsewhere", () => {
    expect(read("  ", {}, true)).toEqual({ ok: true, value: null });
    expect(read("", {})).toEqual({ ok: false, why: "needed" });
    expect(read("", { min: 0 }, false)).toEqual({ ok: false, why: "needed" });
  });
});

describe("a number field's steps", () => {
  it("counts the decimals a number is written with", () => {
    expect([1, 0.5, 0.25, 1250, 1e-7, 2.5e-3, -0.125].map(decimals)).toEqual([0, 1, 2, 0, 7, 4, 3]);
  });

  it("moves by whole steps, without the stray digits of binary fractions", () => {
    expect(stepped(0.1, 1, 0.2, {})).toBe(0.3);
    expect(stepped(0.35, 1, 0.05, {})).toBe(0.4);
    // Down to nothing: 0, and not the -0 a field would write as "-0.00".
    expect(stepped(0.3, -3, 0.1, {})).toBe(0);
    expect(written(stepped(0.3, -3, 0.1, {}), 2)).toBe("0.00");
    expect(stepped(1250, 10, 10, {})).toBe(1350);
    // A value off the steps keeps its own decimals.
    expect(stepped(1.25, 1, 0.5, {})).toBe(1.75);
    expect(stepped(1e-7, 1, 1e-7, {})).toBe(2e-7);
  });

  it("stops at the range's ends", () => {
    const range = { min: -5000, max: 5000 };
    expect(stepped(4995, 1, 10, range)).toBe(5000);
    expect(stepped(5000, 10, 10, range)).toBe(5000);
    expect(stepped(-4995, -1, 10, range)).toBe(-5000);
    expect(stepped(0, -1, 1, { min: 0 })).toBe(0);
  });

  it("writes a value as it is, or with the decimals asked for", () => {
    expect(written(0.35)).toBe("0.35");
    expect(written(0.4, 2)).toBe("0.40");
    expect(written(1250, 0)).toBe("1250");
    expect(written(null)).toBe("");
    expect(written(null, 2)).toBe("");
  });
});
