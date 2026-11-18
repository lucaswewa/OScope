import { describe, expect, it } from "vitest";

import {
  kept,
  linearTicks,
  logTicks,
  loose,
  nearestByX,
  nearestInPlane,
  nice,
  panned,
  place,
  stepOf,
  ticksOf,
  valueAt,
  widened,
  written,
  zoomed,
  type Scale,
} from "@/ui/charts/scale";

const labels = (ticks: { label: string }[]): string[] => ticks.map((tick) => tick.label);

describe("nice numbers (Heckbert, 1990)", () => {
  it("round to 1, 2, 5 or 10 times a power of ten", () => {
    expect([1.4, 1.5, 2.9, 3, 6.9, 7].map((x) => nice(x, true))).toEqual([1, 2, 2, 5, 5, 10]);
    expect(nice(0.034, true)).toBeCloseTo(0.05, 12);
    expect(nice(870, true)).toBe(1000);
  });

  it("go up to the next of them, not rounded", () => {
    expect([1, 1.01, 2, 2.01, 5, 5.01].map((x) => nice(x, false))).toEqual([1, 2, 2, 5, 5, 10]);
    expect(nice(0.0123, false)).toBeCloseTo(0.02, 12);
  });

  it("step an axis's ticks, and widen its range to a tick at each end", () => {
    expect(stepOf(0, 100, 6)).toBe(20);
    expect(stepOf(0, 1, 5)).toBeCloseTo(0.2, 12);
    expect(stepOf(-3, 47, 5)).toBe(10);
    expect(loose(3, 97, 6)).toEqual([0, 100]);
    expect(loose(-0.13, 0.42, 6)).toEqual([-0.2, 0.6]);
  });

  it("widen a range of nothing by one each way, or a tenth of its value", () => {
    expect(widened(5, 5)).toEqual([4, 6]);
    expect(widened(400, 400)).toEqual([360, 440]);
    expect(widened(0, 0)).toEqual([-1, 1]);
    expect(widened(2, 3)).toEqual([2, 3]);
  });
});

describe("a linear axis's ticks", () => {
  it("are round numbers in the range, written with the decimals their step needs", () => {
    expect(labels(linearTicks(0, 100, 6))).toEqual(["0", "20", "40", "60", "80", "100"]);
    expect(labels(linearTicks(0, 1, 5))).toEqual(["0.0", "0.2", "0.4", "0.6", "0.8", "1.0"]);
    expect(labels(linearTicks(-5, 5, 5))).toEqual(["-4", "-2", "0", "2", "4"]);
  });

  it("are right for awkward ranges", () => {
    // A narrow range far from zero, and one of thousandths.
    expect(labels(linearTicks(99.5, 100.5, 5))).toEqual([
      "99.6",
      "99.8",
      "100.0",
      "100.2",
      "100.4",
    ]);
    expect(labels(linearTicks(0.001, 0.0013, 4))).toEqual(["0.0010", "0.0012"]);
    expect(labels(linearTicks(0.001, 0.0013, 7))).toEqual(["0.0010", "0.0011", "0.0012", "0.0013"]);
    // A single value, widened; and millions, in scientific notation.
    expect(labels(linearTicks(7, 7, 3))).toEqual(["6", "7", "8"]);
    expect(labels(linearTicks(1e6, 3e6, 5))).toEqual(["1e6", "1.5e6", "2e6", "2.5e6", "3e6"]);
    // Steps of 0.1 add up exactly, and zero is never "-0".
    expect(linearTicks(-0.3, 0.3, 11).map((tick) => tick.value)).toEqual([
      -0.3, -0.2, -0.1, 0, 0.1, 0.2, 0.3,
    ]);
    expect(labels(linearTicks(-0.3, 0.3, 11))).toContain("0.0");
  });

  it("write a value with a point, and nothing else", () => {
    expect(written(12345.678, 1)).toBe("12345.7");
    expect(written(-0.0001, 2)).toBe("0.00");
    expect(written(-2500000, 0)).toBe("-2.5e6");
    expect(written(1234567.89, 0)).toBe("1.23456789e6");
  });

  it("are one for each so many pixels along the axis, and at least two", () => {
    const scale = (from: number, to: number): Scale => ({
      kind: "linear",
      min: 0,
      max: 100,
      from,
      to,
    });
    expect(ticksOf(scale(0, 800), 80).length).toBe(11);
    expect(ticksOf(scale(300, 0), 50).length).toBe(6);
    expect(ticksOf(scale(0, 20), 80).length).toBeGreaterThanOrEqual(2);
  });
});

describe("a logarithmic axis's ticks", () => {
  it("are at powers of ten, written as the values themselves", () => {
    expect(labels(logTicks(1, 10000))).toEqual(["1", "10", "100", "1000", "10⁴"]);
    expect(labels(logTicks(0.001, 1))).toEqual(["0.001", "0.01", "0.1", "1"]);
    expect(labels(logTicks(1e-6, 1e-4))).toEqual([
      "10⁻⁶",
      "2×10⁻⁶",
      "5×10⁻⁶",
      "10⁻⁵",
      "2×10⁻⁵",
      "5×10⁻⁵",
      "10⁻⁴",
    ]);
  });

  it("are at 2 and 5 times them too, with fewer than three decades", () => {
    expect(labels(logTicks(1, 100))).toEqual(["1", "2", "5", "10", "20", "50", "100"]);
    expect(labels(logTicks(3, 30))).toEqual(["5", "10", "20"]);
    expect(labels(logTicks(1e4, 1e5))).toEqual(["10⁴", "2×10⁴", "5×10⁴", "10⁵"]);
  });

  it("are at every second, fifth or tenth decade over many", () => {
    expect(logTicks(1, 1e20).map((tick) => Math.round(Math.log10(tick.value)))).toEqual([
      0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20,
    ]);
    expect(logTicks(1, 1e30).map((tick) => Math.round(Math.log10(tick.value)))).toEqual([
      0, 5, 10, 15, 20, 25, 30,
    ]);
    expect(logTicks(1e-40, 1e40)).toHaveLength(9);
    expect(labels(logTicks(1e-40, 1)).at(0)).toBe("10⁻⁴⁰");
  });
});

describe("a scale", () => {
  const linear: Scale = { kind: "linear", min: 0, max: 100, from: 50, to: 450 };
  const log: Scale = { kind: "log", min: 1, max: 1000, from: 300, to: 0 };

  it("places a value along it, and finds the value at a pixel", () => {
    expect(place(linear, 25)).toBe(150);
    expect(valueAt(linear, 150)).toBe(25);
    expect(place(log, 10)).toBeCloseTo(200, 9);
    expect(place(log, 1000)).toBeCloseTo(0, 9);
    expect(valueAt(log, 100)).toBeCloseTo(100, 9);
  });

  it("places nothing it can't show on a log scale", () => {
    expect(place(log, 0)).toBeNaN();
    expect(place(log, -3)).toBeNaN();
  });
});

describe("the nearest point", () => {
  it("is found by x among values in order, by a binary search", () => {
    const xs = [0, 1, 2, 4, 8, 16];
    expect(nearestByX(xs, -5)).toBe(0);
    expect(nearestByX(xs, 2.9)).toBe(2);
    expect(nearestByX(xs, 3.1)).toBe(3);
    expect(nearestByX(xs, 3)).toBe(2);
    expect(nearestByX(xs, 99)).toBe(5);
    expect(nearestByX([7], 0)).toBe(0);
    expect(nearestByX([], 0)).toBe(-1);
    // 4,096 of them in a dozen steps.
    const many = Array.from({ length: 4096 }, (_, n) => n * 16);
    expect(nearestByX(many, 1000)).toBe(62);
  });

  it("is found on the screen, for points, by distance in pixels", () => {
    expect(nearestInPlane([0, 10, 20], [0, 10, 0], 11, 8)).toBe(1);
    expect(nearestInPlane([0, 10, 20], [0, 10, 0], 19, 1)).toBe(2);
    expect(nearestInPlane([], [], 0, 0)).toBe(-1);
  });
});

describe("a view's zoom and pan", () => {
  const all = { min: 0, max: 1000 };

  it("zooms about a value, which stays where it was", () => {
    const view = zoomed(all, 2, 250, all, 2);
    expect(view).toEqual({ min: 125, max: 625 });
    expect((250 - view.min) / (view.max - view.min)).toBeCloseTo(0.25, 12);
    expect(zoomed(view, 1.25, 400, all, 2).max - zoomed(view, 1.25, 400, all, 2).min).toBeCloseTo(
      400,
      9,
    );
  });

  it("goes no further out than the data, and no further in than its least", () => {
    expect(zoomed({ min: 100, max: 200 }, 0.01, 150, all, 2)).toEqual(all);
    const closest = zoomed(all, 1e6, 500, all, 2);
    expect(closest.max - closest.min).toBe(2);
    expect(kept({ min: 990, max: 1090 }, all, 2)).toEqual({ min: 900, max: 1000 });
    // A view kept when the data narrowed under it is cut to the data.
    expect(kept({ min: -100, max: 1100 }, all, 2)).toEqual(all);
  });

  it("pans, and stops at the data's ends with its width kept", () => {
    expect(panned({ min: 100, max: 300 }, 50, all, 2)).toEqual({ min: 150, max: 350 });
    expect(panned({ min: 100, max: 300 }, -500, all, 2)).toEqual({ min: 0, max: 200 });
    expect(panned({ min: 700, max: 900 }, 500, all, 2)).toEqual({ min: 800, max: 1000 });
  });
});
