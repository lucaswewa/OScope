// The deep-zoom viewer's logic (W6.4), from the project W6.2 recorded on
// the simulator (a scan, its stitched result, a report): the Deep Zoom
// levels; the look asked of the server; a colour map on a tile's pixels; the
// records it views; and a record's place on the stage.
import { describe, expect, it } from "vitest";

import type { Contents } from "@/features/gallery/project/project";
import recorded from "@/features/gallery/project/recorded-contents.json";
import { tableOf } from "@/features/live/colourMaps";
import { dziLevels, lookKey, lookOf, mapPixels, placementOf, PLAIN, viewable } from "./viewer";

const contents = recorded as unknown as Contents;
const acquisition = contents.acquisitions[0]!;
const result = contents.results[0]!;

describe("the Deep Zoom levels", () => {
  it("run from a pixel to the full size, as the server counts them", () => {
    expect(dziLevels(1, 1)).toBe(1);
    expect(dziLevels(256, 1)).toBe(9);
    expect(dziLevels(257, 10)).toBe(10);
    expect(dziLevels(1024, 768)).toBe(11);
    expect(dziLevels(798, 596)).toBe(11);
  });
});

describe("the look", () => {
  it("asks for the plane alone while it is plain", () => {
    expect(lookOf(PLAIN, 1)).toEqual({ plane: 0 });
  });

  it("keeps the plane among the image's, and black below white", () => {
    expect(lookOf({ ...PLAIN, plane: 7.6 }, 9).plane).toBe(8);
    expect(lookOf({ ...PLAIN, plane: 12 }, 9).plane).toBe(8);
    expect(lookOf({ ...PLAIN, plane: -1 }, 9).plane).toBe(0);
    const crossed = lookOf({ ...PLAIN, black: 0.6, white: 0.4 }, 1);
    expect(crossed.black).toBeLessThan(crossed.white!);
    expect(lookOf({ ...PLAIN, white: 0.5, gamma: 2 }, 1)).toEqual({
      plane: 0,
      black: 0,
      white: 0.5,
      gamma: 2,
    });
  });

  it("tells one look's tiles from another's, and a colour map's", () => {
    const keys = [
      lookKey({ plane: 0 }, "none"),
      lookKey({ plane: 1 }, "none"),
      lookKey({ plane: 0, black: 0, white: 0.5, gamma: 1 }, "none"),
      lookKey({ plane: 0 }, "viridis"),
    ];
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("maps a tile's grey through a colour map's colours, its alpha its own", () => {
    const table = tableOf("hilo")!;
    const pixels = new Uint8ClampedArray([0, 0, 0, 255, 128, 128, 128, 200, 255, 255, 255, 255]);
    mapPixels(pixels, table);
    expect([...pixels]).toEqual([0, 0, 255, 255, 128, 128, 128, 200, 255, 0, 0, 255]);
  });
});

describe("the records viewed", () => {
  it("are those with an image: acquisitions, then results", () => {
    expect(viewable(contents).map((one) => one.record.id)).toEqual([acquisition.id, result.id]);
    const imageless = { ...contents, results: [{ ...result, image: null }] };
    expect(viewable(imageless).map((one) => one.kind)).toEqual(["acquisition"]);
  });
});

describe("a record's place on the stage", () => {
  const pixel = acquisition.image.pixel_size!.x;
  const { width, height } = acquisition.image;

  it("is an acquisition's plane's, about its frame's place", () => {
    for (const plane of [0, 3]) {
      const at = acquisition.frames[plane]!.position!;
      expect(placementOf({ kind: "acquisition", record: acquisition }, contents, plane)).toEqual({
        x_um: at.x - (width * pixel) / 2,
        y_um: at.y - (height * pixel) / 2,
        pixel_um: pixel,
        width,
        height,
      });
    }
  });

  it("is a stitched result's, at the first frame of what it was made from", () => {
    const at = acquisition.frames[0]!.position!;
    const placed = placementOf({ kind: "result", record: result }, contents)!;
    expect(placed.x_um).toBeCloseTo(at.x - (width * pixel) / 2, 6);
    expect(placed.y_um).toBeCloseTo(at.y - (height * pixel) / 2, 6);
    expect([placed.width, placed.height]).toEqual([result.image!.width, result.image!.height]);
  });

  it("is the stage's origin, its pixels a µm, where nothing places it", () => {
    const alone = { ...acquisition, frames: [], image: { ...acquisition.image, pixel_size: null } };
    expect(placementOf({ kind: "acquisition", record: alone }, contents)).toEqual({
      x_um: 0,
      y_um: 0,
      pixel_um: 1,
      width,
      height,
    });
  });
});
