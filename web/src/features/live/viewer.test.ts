// The live viewer's arithmetic (W3.1): the pointer on the canvas, the zoom's
// steps, the pixel under the pointer and its place at the sample, and the
// scale bar's round lengths.
import { describe, expect, it } from "vitest";

import {
  fromCentre,
  onCanvas,
  oneToOne,
  percent,
  pixelAt,
  roundLength,
  scaleBar,
  STEP,
  wheelFactor,
} from "./viewer";

/** A frame of the MIChrome 20's sensor, whole. */
const SENSOR = { width: 5472, height: 3648 };
const EXTENT = { x: 0, y: 0, ...SENSOR };
const CANVAS = { width: 1600, height: 1000 };

describe("the pointer, the pane and the sensor", () => {
  it("takes a point in the canvas's box to its device pixels", () => {
    expect(onCanvas([10, 20], { width: 800, height: 500 }, CANVAS)).toEqual([20, 40]);
  });

  it("finds the sensor's pixel under the pointer, and none outside the frame", () => {
    // Fitted: a sensor pixel on 0.25 of a device pixel, the sensor's centre at the canvas's.
    const view = { centre: [2736, 1824] as [number, number], scale: 0.25 };
    expect(pixelAt(view, [800, 500], CANVAS, EXTENT)).toEqual([2736, 1824]);
    expect(pixelAt(view, [801, 499], CANVAS, EXTENT)).toEqual([2740, 1820]);
    expect(pixelAt(view, [0, 0], CANVAS, EXTENT)).toBeUndefined();
    // Zoomed in, each device pixel a quarter of a sensor pixel.
    const near = { centre: [100.5, 200.5] as [number, number], scale: 4 };
    expect(pixelAt(near, [800, 500], CANVAS, EXTENT)).toEqual([100, 200]);
    expect(pixelAt(near, [804, 496], CANVAS, EXTENT)).toEqual([101, 199]);
  });

  it("places a pixel at the sample, from the sensor's centre, along its axes", () => {
    // 3.45 µm pixels through a 10× objective: 0.345 µm at the sample.
    const [x, y] = fromCentre([2736, 1824], SENSOR, 0.345);
    expect([x, y].map((v) => +v.toFixed(4))).toEqual([0.1725, 0.1725]);
    const [left, top] = fromCentre([0, 0], SENSOR, 0.345);
    expect([+left.toFixed(1), +top.toFixed(1)]).toEqual([-943.7, -629.1]);
  });
});

describe("the zoom", () => {
  it("steps by a quarter, a notch of the wheel a step, towards the user in", () => {
    expect(wheelFactor(-100)).toBe(STEP);
    expect(wheelFactor(100)).toBe(1 / STEP);
    expect(wheelFactor(-50)).toBeCloseTo(Math.sqrt(STEP));
    expect(wheelFactor(0)).toBe(1);
    // A hard flick is four notches at most.
    expect(wheelFactor(-5000)).toBe(STEP ** 4);
  });

  it("shows each sensor pixel on one device pixel at one to one, the point under the pointer kept", () => {
    const view = { centre: [2736, 1824] as [number, number], scale: 0.25 };
    const one = oneToOne(view, [800, 500], EXTENT, CANVAS);
    expect(one).toEqual({ centre: [2736, 1824], scale: 1 });
    expect(percent(one)).toBe(100);
    expect(percent(view)).toBe(25);
  });
});

describe("the scale bar", () => {
  it("is 1, 2 or 5 times a power of ten, the longest that fits", () => {
    const lengths = [0.7, 3, 5, 37, 50, 99, 100, 1234, 4999].map(roundLength);
    expect(lengths).toEqual([0.5, 2, 5, 20, 50, 50, 100, 1000, 2000]);
  });

  it("shortens as the image is zoomed, and is as long on the screen as at the sample", () => {
    // A sensor pixel 1 µm at the sample; 160 CSS pixels at most.
    const far = scaleBar(1, { centre: [0, 0], scale: 0.5 }, 1, 160)!;
    expect(far).toEqual({ length: 200, width: 100 });
    const near = scaleBar(1, { centre: [0, 0], scale: 2 }, 1, 160)!;
    expect(near).toEqual({ length: 50, width: 100 });
    // On a screen of two device pixels to a CSS pixel, a CSS pixel spans twice as much.
    expect(scaleBar(1, { centre: [0, 0], scale: 2 }, 2, 160)).toEqual({ length: 100, width: 100 });
  });

  it("isn't drawn without an objective fitted", () => {
    expect(scaleBar(null, { centre: [0, 0], scale: 1 }, 1, 160)).toBeUndefined();
    expect(scaleBar(undefined, { centre: [0, 0], scale: 1 }, 1, 160)).toBeUndefined();
  });
});
