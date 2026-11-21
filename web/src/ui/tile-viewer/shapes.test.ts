// Shapes over the tile viewer's image (W5.1): a rectangle from two corners,
// whichever way round, its corners as a shape, and a shape's points in the
// image's pixels.
import { describe, expect, it } from "vitest";

import { between, pointsIn, rectShape } from "./shapes";

describe("shapes on the stage", () => {
  it("make a rectangle of two corners, whichever way round", () => {
    const rect = { x: 10, y: 20, width: 30, height: 40 };
    expect(between({ x: 10, y: 20 }, { x: 40, y: 60 })).toEqual(rect);
    expect(between({ x: 40, y: 60 }, { x: 10, y: 20 })).toEqual(rect);
    expect(between({ x: 40, y: 20 }, { x: 10, y: 60 })).toEqual(rect);
  });

  it("go round a rectangle's corners, from its least x and y", () => {
    expect(rectShape("region", { x: 1, y: 2, width: 3, height: 4 })).toEqual({
      kind: "region",
      points: [
        { x: 1, y: 2 },
        { x: 4, y: 2 },
        { x: 4, y: 6 },
        { x: 1, y: 6 },
      ],
    });
  });

  it("are drawn in the image's pixels, from where it lies on the stage", () => {
    const at = { x_um: 1000, y_um: 500, pixel_um: 10, width: 100, height: 50 };
    const shape = rectShape("frame", { x: 1100, y: 600, width: 50, height: 20 });
    expect(pointsIn(at, shape)).toBe("10.00,10.00 15.00,10.00 15.00,12.00 10.00,12.00");
  });
});
