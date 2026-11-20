import { describe, expect, it } from "vitest";

import { fieldOf, placementOf, serverLevel, type MosaicState } from "./mosaic";

const state = {
  mosaic: 2,
  drawing: true,
  roaming: true,
  canvas: { x_um: -1, y_um: -1, pixel_um: 0.48, width: 208_336, height: 156_252 },
  levels: 11,
  tile: 256,
} as MosaicState;

describe("the live mosaic", () => {
  it("places its canvas on the stage, once there is one", () => {
    expect(placementOf(state)).toEqual(state.canvas);
    expect(placementOf({ ...state, canvas: null })).toBeUndefined();
    expect(placementOf(undefined)).toBeUndefined();
  });

  it("counts its levels from the finest, as the viewer counts from the coarsest", () => {
    expect([10, 0, 7].map((level) => serverLevel(level, 11))).toEqual([0, 10, 3]);
  });

  it("marks the field of view about the stage, the camera's region through the objective", () => {
    const field = fieldOf({ x: 40_000, y: 12_000 }, { width: 960, height: 648 }, 0.24)!;
    const round = (value: number) => Math.round(value * 1e6) / 1e6;
    expect([field.x, field.y, field.width, field.height].map(round)).toEqual([
      39_884.8, 11_922.24, 230.4, 155.52,
    ]);
    expect(fieldOf({ x: 0, y: 0 }, { width: 960, height: 648 }, null)).toBeUndefined();
    expect(fieldOf(undefined, { width: 960, height: 648 }, 0.24)).toBeUndefined();
  });
});
