// The overlays' geometry (W3.5): a rectangle from two corners, moved and
// resized within the frame; the handles' keys; a region on the camera's
// grid; the grid's lines; and the shapes held in place under zoom and pan.
import { describe, expect, it } from "vitest";

import { placed, sensorAt, zoomed, panned, type View } from "@/api/preview/view";
import { cornerOf, gridLines, gridStep, moved, nudge, onGrid, resized, spanned } from "./geometry";

/** The small region the tests stream: 960 × 648 about the sensor's centre. */
const FRAME = { x: 2256, y: 1500, width: 960, height: 648 };

describe("a rectangle", () => {
  it("spans two corners, either way round, in whole pixels", () => {
    const rect = { x: 2300, y: 1600, width: 101, height: 51 };
    expect(spanned([2300.4, 1600.2], [2400.5, 1650.9], FRAME)).toEqual(rect);
    expect(spanned([2400.5, 1650.9], [2300.4, 1600.2], FRAME)).toEqual(rect);
  });

  it("stays within the frame, drawn past it or moved past it", () => {
    expect(spanned([2000, 1400], [2300, 1600], FRAME)).toEqual({
      x: 2256,
      y: 1500,
      width: 44,
      height: 100,
    });
    const rect = { x: 3000, y: 2000, width: 100, height: 100 };
    expect(moved(rect, [500, -900], FRAME)).toEqual({ x: 3116, y: 1500, width: 100, height: 100 });
  });

  it("is resized by a corner, the opposite one kept, past it if dragged so", () => {
    const rect = { x: 2400, y: 1600, width: 100, height: 100 };
    expect(resized(rect, "bottomRight", [2550, 1650], FRAME)).toEqual({
      x: 2400,
      y: 1600,
      width: 150,
      height: 50,
    });
    // Dragged past the opposite corner, it turns over, and the corners swap.
    expect(resized(rect, "topLeft", [2600, 1750], FRAME)).toEqual({
      x: 2500,
      y: 1700,
      width: 100,
      height: 50,
    });
    expect(cornerOf(rect, "topRight")).toEqual([2500, 1600]);
  });
});

describe("the handles' keys", () => {
  it("move a pixel with an arrow, ten with Shift, and leave other keys alone", () => {
    expect(nudge("ArrowLeft", false)).toEqual([-1, 0]);
    expect(nudge("ArrowDown", true)).toEqual([0, 10]);
    expect(nudge("Enter", false)).toBeUndefined();
    expect(nudge("toString", false)).toBeUndefined();
  });
});

describe("the camera's region from an ROI", () => {
  it("is rounded out to the camera's grid, and at least its least size", () => {
    const grid = { x: 12, y: 12, least: 48 };
    expect(onGrid({ x: 2305, y: 1601, width: 100, height: 20 }, grid)).toEqual({
      x: 2304,
      y: 1596,
      width: 108,
      height: 48,
    });
  });
});

describe("the grid", () => {
  it("is spaced at a round number of the sensor's pixels, some 120 canvas pixels apart", () => {
    expect(gridStep(1)).toBe(100);
    expect(gridStep(0.5)).toBe(200);
    expect(gridStep(4)).toBe(20);
  });

  it("falls on the sensor's multiples of its step, across the frame", () => {
    const { xs, ys } = gridLines(FRAME, 200);
    expect(xs).toEqual([2400, 2600, 2800, 3000, 3200]);
    expect(ys).toEqual([1600, 1800, 2000]);
  });
});

describe("a shape under zoom and pan", () => {
  it("keeps its place on the sensor: drawn again, it is where the image shows it", () => {
    const canvas = { width: 800, height: 600 };
    const rect = { x: 2600, y: 1700, width: 40, height: 30 };
    let view: View = { centre: [2736, 1824], scale: 0.8 };
    for (const next of [
      (v: View) => zoomed(v, 2, [100, 50], FRAME, canvas),
      (v: View) => panned(v, [60, -40], FRAME, canvas),
    ]) {
      view = next(view);
      const drawn = placed(view, rect, canvas);
      // Its corner on the canvas is the sensor's point under it.
      expect(sensorAt(view, [drawn.x, drawn.y], canvas)).toEqual([
        expect.closeTo(rect.x, 9),
        expect.closeTo(rect.y, 9),
      ]);
      expect(drawn.width / rect.width).toBeCloseTo(view.scale, 9);
    }
  });
});
