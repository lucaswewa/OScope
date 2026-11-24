import { describe, expect, it } from "vitest";

import {
  byObjective,
  exportable,
  fileOf,
  mapOf,
  objectiveOf,
  residualsOf,
  type Summary,
} from "@/features/settings/calibration/calibration";

/** A set's summary, of `key`. */
const set = (key: string, id = key): Summary => ({
  id,
  kind: key.split("/")[0]!,
  key,
  version: 1,
  created: "2026-11-24T10:00:00Z",
  active: true,
  headline: key,
});

describe("the sets by objective", () => {
  it("are a geometry's and a flat field's, by their keys; the rest are the instrument's", () => {
    expect(objectiveOf("geometry/10x/1")).toBe("10x");
    expect(objectiveOf("flat/50x/blue 488+transmitted/2")).toBe("50x");
    expect(objectiveOf("dark/full/1/8")).toBeUndefined();
    expect(objectiveOf("colour/transmitted")).toBeUndefined();
    expect(objectiveOf("stage")).toBeUndefined();
  });

  it("go in the objectives' order, by their numbers, then the instrument's, each as listed", () => {
    const listed = [
      set("stage"),
      set("geometry/20x/1"),
      set("flat/10x/transmitted/1"),
      set("geometry/5x/1"),
      set("dark/full/1/8"),
      set("geometry/10x/1"),
    ];
    const groups = byObjective(listed).map((g) => [g.objective, g.sets.map((s) => s.key)]);
    expect(groups).toEqual([
      ["5x", ["geometry/5x/1"]],
      ["10x", ["flat/10x/transmitted/1", "geometry/10x/1"]],
      ["20x", ["geometry/20x/1"]],
      [undefined, ["stage", "dark/full/1/8"]],
    ]);
    expect(byObjective([])).toEqual([]);
  });

  it("are exported, all but the dark frames and flat fields, whose images stay", () => {
    const kinds = ["geometry", "stage", "colour", "dark", "flat"];
    expect(kinds.filter((kind) => exportable({ kind }))).toEqual(["geometry", "stage", "colour"]);
  });

  it("are exported to files named by their keys and versions", () => {
    expect(fileOf({ key: "flat/10x/blue 488+transmitted/1", version: 3 })).toBe(
      "oscope-calibration-flat-10x-blue-488+transmitted-1-v3.json",
    );
    expect(fileOf({ key: "dark/full/1.5/8", version: 1 })).toBe(
      "oscope-calibration-dark-full-1.5-8-v1.json",
    );
  });
});

describe("a flat field's map", () => {
  // A table whose colour at each level is the level itself, in red.
  const table = new Uint8Array(256 * 3).map((_, i) => (i % 3 === 0 ? i / 3 : 0));

  it("runs from its lowest block to its highest, each block's channels' mean", () => {
    const image = {
      width: 3,
      height: 1,
      channels: 2,
      block: 4,
      values: [0.5, 0.5, 1.25, 0.75, 0.75, 0.75],
    };
    const map = mapOf(image, table);
    expect([map.low, map.high]).toEqual([0.5, 1]);
    // Its levels: 0.5, 1 and 0.75, at 0, 255 and halfway.
    expect([...map.rgba]).toEqual([0, 0, 0, 255, 255, 0, 0, 255, 128, 0, 0, 255]);
  });

  it("is the table's first colour where it is even, and nothing where it is empty", () => {
    const even = mapOf({ width: 2, height: 1, channels: 1, block: 1, values: [1, 1] }, table);
    expect([even.low, even.high, ...even.rgba]).toEqual([1, 1, 0, 0, 0, 255, 0, 0, 0, 255]);
    const empty = mapOf({ width: 0, height: 0, channels: 1, block: 1, values: [] }, table);
    expect([empty.rgba.length, empty.low, empty.high]).toEqual([0, 0, 0]);
  });
});

describe("the geometry's residuals", () => {
  it("are points of x against y, in order of x, their root mean square the fit's error", () => {
    const { series, reach, rms } = residualsOf(
      [
        [0.3, -0.4],
        [0, 0],
        [-0.6, 0.8],
      ],
      "Moves",
    );
    expect(series).toEqual([
      { name: "Moves", kind: "points", x: [-0.6, 0, 0.3], y: [0.8, 0, -0.4] },
    ]);
    // √((0.25 + 0 + 1) / 3)
    expect(rms).toBeCloseTo(Math.sqrt(1.25 / 3), 12);
    // The furthest, 0.8, with a quarter more.
    expect(reach).toBeCloseTo(1, 12);
  });

  it("are none in a set made before them, on axes of their own", () => {
    const none = residualsOf([], "Moves");
    expect([none.series[0]!.x, none.rms, none.reach]).toEqual([[], 0, 0.1]);
  });
});
