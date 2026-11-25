// The memory's trend (W10.3, `tests/perf/slope.ts`): Theil–Sen's slope, its
// median not moved by a few samples far off the line, and the series' steady
// part after its first quarter.
import { describe, expect, it } from "vitest";

import { slopeOf, steady } from "../perf/slope.ts";

describe("a series' slope", () => {
  it("is a line's own slope", () => {
    const line = Array.from({ length: 10 }, (_, x) => [x, 3 + 2 * x] as const);
    expect(slopeOf(line)).toBe(2);
    expect(slopeOf(line.map(([x, y]) => [x, -y] as const))).toBe(-2);
  });

  it("is flat for a steady series, though a few samples are far off it", () => {
    const flat = Array.from({ length: 20 }, (_, x) => [x, 100] as const);
    const spiked = flat.map(([x, y]) => [x, x % 7 === 3 ? y + 80 : y] as const);
    expect(slopeOf(spiked)).toBe(0);
    // A mean of the slopes would follow the spikes.
    expect(slopeOf([...spiked, [20, 400]])).toBe(0);
  });

  it("finds a slow leak under noise", () => {
    const noise = [0.4, -0.3, 0.1, -0.5, 0.2, 0.3, -0.1, -0.2, 0.5, 0];
    const leak = noise.map((n, x) => [x, 50 + 0.5 * x + n] as const);
    expect(slopeOf(leak)).toBeCloseTo(0.5, 1);
  });

  it("is NaN without two points at distinct places", () => {
    expect(slopeOf([])).toBeNaN();
    expect(slopeOf([[1, 1]])).toBeNaN();
    expect(
      slopeOf([
        [1, 1],
        [1, 2],
      ]),
    ).toBeNaN();
  });
});

describe("a series' steady part", () => {
  it("leaves out its first quarter", () => {
    expect(steady([1, 2, 3, 4, 5, 6, 7, 8])).toEqual([3, 4, 5, 6, 7, 8]);
    expect(steady([1, 2, 3])).toEqual([1, 2, 3]);
  });
});
