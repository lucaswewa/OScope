// The overview, for the tile viewer (W5.1): its pixels from the server's
// base64 RGB, its place on the stage, and the blank stage before one.
import { describe, expect, it } from "vitest";

import { blankOf, placementOf, rgbaOf, type OverviewView } from "./overview";

describe("the overview", () => {
  it("is opaque RGBA from the server's RGB, a byte each, by rows", () => {
    const rgb = btoa(String.fromCharCode(1, 2, 3, 250, 251, 252, 7, 8, 9, 0, 0, 0));
    expect([...rgbaOf(rgb, 2, 2)]).toEqual([
      1, 2, 3, 255, 250, 251, 252, 255, 7, 8, 9, 255, 0, 0, 0, 255,
    ]);
  });

  it("lies on the stage from its first pixel's corner, a pixel its pixel_um", () => {
    const view = {
      origin: { x: 22_000, y: 500 },
      pixel_um: 50,
      width: 1000,
      height: 480,
    } as OverviewView;
    expect(placementOf(view)).toEqual({
      x_um: 22_000,
      y_um: 500,
      pixel_um: 50,
      width: 1000,
      height: 480,
    });
  });

  it("is, before one, the stage's travel, blank, a pixel 100 µm", async () => {
    const { image, placement } = blankOf({
      x: { min: 0, max: 100_000 },
      y: { min: 0, max: 75_000 },
    });
    expect(placement).toEqual({ x_um: 0, y_um: 0, pixel_um: 100, width: 1000, height: 750 });
    expect([image.width, image.height, image.levels, image.tileSize]).toEqual([1000, 750, 1, 1024]);
    await expect(image.load(0, 0, 0, new AbortController().signal)).resolves.toBeUndefined();
  });
});
