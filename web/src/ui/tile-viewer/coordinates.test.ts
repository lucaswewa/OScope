import { describe, expect, it } from "vitest";

import {
  fieldAbout,
  imageToStage,
  stageRectToViewport,
  stageToImage,
  stageToViewport,
  viewportRectToStage,
  viewportToStage,
  type Placement,
} from "./coordinates";

/** A canvas over the simulated stage's travel, its pixel 0.5 µm, as a live mosaic's is. */
const canvas: Placement = { x_um: 0, y_um: 0, pixel_um: 0.5, width: 200_000, height: 150_000 };
/** One whose corner isn't the stage's, its pixel not a round number. */
const offset: Placement = { x_um: -1, y_um: 2_500, pixel_um: 0.4537, width: 4_096, height: 3_000 };

describe("the tile viewer's coordinates", () => {
  it("place the stage's µm on the image's pixels, and back", () => {
    expect(stageToImage(canvas, { x: 40_000, y: 12_000 })).toEqual({ x: 80_000, y: 24_000 });
    expect(imageToStage(canvas, { x: 80_000, y: 24_000 })).toEqual({ x: 40_000, y: 12_000 });
    const back = imageToStage(offset, stageToImage(offset, { x: 1_234.5, y: 3_000.25 }));
    expect(back.x).toBeCloseTo(1_234.5, 9);
    expect(back.y).toBeCloseTo(3_000.25, 9);
    expect(stageToImage(offset, { x: -1, y: 2_500 })).toEqual({ x: 0, y: 0 });
  });

  it("measure the viewport in the image's widths, down as well as across", () => {
    expect(stageToViewport(canvas, { x: 100_000, y: 75_000 })).toEqual({ x: 1, y: 0.75 });
    expect(viewportToStage(canvas, { x: 0.5, y: 0.25 })).toEqual({ x: 50_000, y: 25_000 });
    const there = viewportToStage(offset, stageToViewport(offset, { x: 700, y: 3_100 }));
    expect([there.x, there.y].map((v) => Math.round(v * 1e6) / 1e6)).toEqual([700, 3_100]);
  });

  it("give a field of view about its centre, and as a rectangle of the viewport", () => {
    const field = fieldAbout({ x: 40_000, y: 12_000 }, 1_000, 600);
    expect(field).toEqual({ x: 39_500, y: 11_700, width: 1_000, height: 600 });
    expect(stageRectToViewport(canvas, field)).toEqual({
      x: 0.395,
      y: 0.117,
      width: 0.01,
      height: 0.006,
    });
  });

  it("say a view of the viewport as a rectangle of the stage, the one it came from (W6.4)", () => {
    const field = { x: 700, y: 3_100, width: 250, height: 125 };
    const back = viewportRectToStage(offset, stageRectToViewport(offset, field));
    for (const key of ["x", "y", "width", "height"] as const)
      expect(back[key]).toBeCloseTo(field[key], 6);
    expect(viewportRectToStage(canvas, { x: 0.5, y: 0.25, width: 0.1, height: 0.05 })).toEqual({
      x: 50_000,
      y: 25_000,
      width: 10_000,
      height: 5_000,
    });
  });
});
