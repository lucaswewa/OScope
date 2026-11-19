// The view onto the frame (W1.5): fitted, zoomed about a point, panned and
// kept to the frame; where a region is drawn; and the region asked for.
import { describe, expect, it } from "vitest";

import {
  fitScale,
  fitted,
  kept,
  MOST_SCALE,
  panned,
  placed,
  roiFor,
  sensorAt,
  zoomed,
} from "./view.ts";

/** The simulated camera's 20 MP sensor, and a 1920 × 1280 canvas. */
const SENSOR = { x: 0, y: 0, width: 5472, height: 3648 };
const CANVAS = { width: 1920, height: 1280 };

describe("the view", () => {
  it("fits the whole frame to the canvas, centred, and asks for no region", () => {
    const view = fitted(SENSOR, CANVAS);
    expect(view.centre).toEqual([2736, 1824]);
    expect(view.scale).toBeCloseTo(1920 / 5472);
    expect(placed(view, SENSOR, CANVAS)).toEqual({ x: 0, y: 0, width: 1920, height: 1280 });
    expect(roiFor(view, SENSOR, CANVAS)).toBeUndefined();
  });

  it("fits a frame that is part of the sensor where it lies", () => {
    const part = { x: 2256, y: 1464, width: 960, height: 720 };
    const view = fitted(part, CANVAS);
    expect(view.centre).toEqual([2736, 1824]);
    expect(view.scale).toBeCloseTo(1280 / 720);
    expect(placed(view, part, CANVAS).x).toBeCloseTo((1920 - 960 * (1280 / 720)) / 2);
  });

  it("zooms about a point, which stays under it, and then asks for the region it shows", () => {
    const view = fitted(SENSOR, CANVAS);
    const at: [number, number] = [480, 320];
    const under = sensorAt(view, at, CANVAS);
    const near = zoomed(view, 4, at, SENSOR, CANVAS);
    expect(near.scale).toBeCloseTo(view.scale * 4);
    sensorAt(near, at, CANVAS).forEach((value, axis) => expect(value).toBeCloseTo(under[axis]!));
    expect(roiFor(near, SENSOR, CANVAS)).toEqual({ x: 1026, y: 684, width: 1368, height: 912 });
  });

  it("keeps to the frame: no smaller than fitted, no larger than the most, no edge inside the canvas", () => {
    const view = fitted(SENSOR, CANVAS);
    expect(zoomed(view, 0.25, [0, 0], SENSOR, CANVAS)).toEqual(view);
    expect(zoomed(view, 1000, [960, 640], SENSOR, CANVAS).scale).toBe(MOST_SCALE);
    const near = zoomed(view, 4, [960, 640], SENSOR, CANVAS);
    const left = panned(near, [100_000, 0], SENSOR, CANVAS);
    expect(placed(left, SENSOR, CANVAS).x).toBeCloseTo(0);
    const top = panned(near, [0, 100_000], SENSOR, CANVAS);
    expect(placed(top, SENSOR, CANVAS).y).toBeCloseTo(0);
    expect(kept({ centre: [-50, 99_999], scale: 0 }, SENSOR, CANVAS)).toEqual(view);
  });

  it("pans with the pointer: the image moves as far as it does", () => {
    const near = zoomed(fitted(SENSOR, CANVAS), 4, [960, 640], SENSOR, CANVAS);
    const moved = panned(near, [140, -70], SENSOR, CANVAS);
    const before = placed(near, SENSOR, CANVAS);
    const after = placed(moved, SENSOR, CANVAS);
    expect([after.x - before.x, after.y - before.y]).toEqual(
      [140, -70].map((n) => expect.closeTo(n)),
    );
  });

  it("centres a side that fits whole, however it is panned", () => {
    // Narrow and tall: zoomed by half again, the frame's height still fits.
    const tall = { width: 400, height: 1280 };
    const view = zoomed(fitted(SENSOR, tall), 1.5, [200, 640], SENSOR, tall);
    expect(view.scale).toBeCloseTo(fitScale(SENSOR, tall) * 1.5);
    expect(panned(view, [0, 500], SENSOR, tall).centre[1]).toBe(1824);
    expect(panned(view, [300, 0], SENSOR, tall).centre[0]).toBeLessThan(2736);
  });
});
