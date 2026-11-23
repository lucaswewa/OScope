// A volume's transfer function (W7.8): its points kept in order and within
// 0 to 1, dragged with its ends held, mapped over a volume's range; and the
// range, of the samples that are numbers.
import { describe, expect, it } from "vitest";

import { moved, nodesOf, ordered, rangeOf, START } from "./transfer";

describe("the transfer function", () => {
  it("keeps its points within 0 to 1, in order of value", () => {
    expect(
      ordered([
        { value: 0.7, opacity: 1.4 },
        { value: -0.2, opacity: 0.3 },
        { value: 0.4, opacity: -1 },
      ]),
    ).toEqual([
      { value: 0, opacity: 0.3 },
      { value: 0.4, opacity: 0 },
      { value: 0.7, opacity: 1 },
    ]);
  });

  it("maps its points over a volume's range, the ends to its least and most", () => {
    expect(nodesOf(START, [1_000, 6_000])).toEqual([
      [1_000, 0],
      [2_000, 0],
      [6_000, 0.5],
    ]);
    expect(nodesOf([{ value: 0.5, opacity: 0.25 }], [-2, 2])).toEqual([[0, 0.25]]);
  });

  it("moves a point within the line, and an end only up and down", () => {
    expect(moved(START, 1, { value: 0.6, opacity: 0.3 })).toEqual([
      { value: 0, opacity: 0 },
      { value: 0.6, opacity: 0.3 },
      { value: 1, opacity: 0.5 },
    ]);
    expect(moved(START, 0, { value: 0.5, opacity: 0.1 })[0]).toEqual({ value: 0, opacity: 0.1 });
    expect(moved(START, 2, { value: 0.3, opacity: 2 })[2]).toEqual({ value: 1, opacity: 1 });
  });

  it("spans the samples that are numbers, and a flat volume over one", () => {
    expect(rangeOf(Float32Array.from([3, NaN, -1, Infinity, 7]))).toEqual([-1, 7]);
    expect(rangeOf([5, 5, 5])).toEqual([5, 6]);
    expect(rangeOf([NaN])).toEqual([0, 1]);
    expect(rangeOf([])).toEqual([0, 1]);
  });
});
