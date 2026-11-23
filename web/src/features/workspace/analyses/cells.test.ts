// A cell count's result and stains separated (W7.6): the cells' table read;
// their outlines placed on the image; which outlines a view needs; the
// histograms' bins; the count per mm²; and the stains and DAB's score.
import { describe, expect, it } from "vitest";

import { gridOf, histogramOf, outlinesOf, perMm2, rowsOf, visible, type Outline } from "./cells";
import { deconvolved, scoreOf, stainsOf, VECTORS } from "./stains";

const square = (label: number, x: number, y: number, side = 10): Outline => ({
  label,
  ring: [],
  bounds: { x, y, width: side, height: side },
});

describe("a cell count", () => {
  it("reads its table, numbers as numbers", () => {
    const csv = "label,area,circularity,x,y\r\n1,50.5,0.9,10,20\r\n2,12,1,,b\r\n";
    expect(rowsOf(csv)).toEqual([
      { label: 1, area: 50.5, circularity: 0.9, x: 10, y: 20 },
      { label: 2, area: 12, circularity: 1, x: "", y: "b" },
    ]);
  });

  it("places its outlines on the image: by the recipe's pixel, the region's corner and the image's", () => {
    const geojson = {
      features: [
        {
          geometry: {
            type: "Polygon",
            coordinates: [
              [
                [0, 0],
                [2, 0],
                [2, 1],
                [0, 0],
              ],
            ],
          },
          properties: { label: 7 },
        },
        { geometry: { type: "Point", coordinates: [1, 1] }, properties: { label: 8 } },
      ],
    };
    const [outline, ...more] = outlinesOf(geojson, {
      pixelUm: [0.5, 0.5],
      at: [100, 10],
      imageUm: 0.25,
    });
    expect(more).toEqual([]);
    // (0, 0) µm is the region's corner, pixel (100, 10); 2 µm across is 4 pixels, a µm of the image's 4.
    expect(outline).toEqual({
      label: 7,
      ring: [
        { x: 25, y: 2.5 },
        { x: 26, y: 2.5 },
        { x: 26, y: 3 },
        { x: 25, y: 2.5 },
      ],
      bounds: { x: 25, y: 2.5, width: 1, height: 0.5 },
    });
  });

  it("draws only the outlines a view needs, each once, the grid's cells crossed or not", () => {
    const outlines = [
      square(1, 0, 0),
      square(2, 95, 95),
      square(3, 300, 300),
      square(4, 150, 40, 200),
    ];
    const grid = gridOf(outlines, 100);
    const labels = (view: { x: number; y: number; width: number; height: number }) =>
      visible(grid, view).map((one) => one.label);
    expect(labels({ x: 0, y: 0, width: 50, height: 50 })).toEqual([1]);
    // One across four of the grid's cells: once.
    expect(labels({ x: 90, y: 90, width: 20, height: 20 })).toEqual([2]);
    // In a cell the view meets, and not in the view: not drawn.
    expect(labels({ x: 0, y: 30, width: 20, height: 20 })).toEqual([]);
    expect(labels({ x: 100, y: 0, width: 400, height: 400 })).toEqual([2, 3, 4]);
    // A view of everything: each outline tested, no loop over endless cells.
    expect(labels({ x: 0, y: 0, width: Infinity, height: Infinity })).toEqual([1, 2, 3, 4]);
  });

  it("bins its measures from the least to the greatest, the greatest in the last bin", () => {
    expect(histogramOf([0, 1, 2, 3, 4, 10], 5)).toEqual({
      edges: [0, 2, 4, 6, 8],
      counts: [2, 2, 1, 0, 1],
      width: 2,
    });
    expect(histogramOf([5, 5, 5], 4)).toEqual({
      edges: [5, 6, 7, 8],
      counts: [3, 0, 0, 0],
      width: 1,
    });
    expect(histogramOf([], 4)).toEqual({ edges: [], counts: [], width: 1 });
  });

  it("counts per mm²", () => {
    expect(perMm2(48, 384 * 288)).toBeCloseTo(434.03, 2);
    expect(perMm2(5, 0)).toBe(0);
  });
});

describe("stains separated", () => {
  it("are named and given their vectors, by the recipe's choice", () => {
    expect(stainsOf("hdab")).toEqual({
      names: ["Hematoxylin", "DAB", "Residual"],
      vectors: [VECTORS.Hematoxylin, VECTORS.DAB],
    });
    expect(stainsOf(undefined).names).toEqual(["Hematoxylin", "Eosin", "Residual"]);
    expect(
      stainsOf({
        vectors: [
          [1, 0, 0],
          [0, 1, 0],
          [0, 0, 1],
        ],
      }).names,
    ).toEqual(["Stain 1", "Stain 2", "Stain 3"]);
    const recipe = { nodes: [{ node: "stain.deconvolve", params: { stains: "hdab" } }] };
    expect(deconvolved(recipe)?.names[1]).toBe("DAB");
    expect(deconvolved({ nodes: [{ node: "filter.gaussian" }] })).toBeUndefined();
  });

  it("score DAB from its row", () => {
    const row = {
      glass: 10,
      negative: 5,
      weak: 1,
      moderate: 2,
      strong: 3,
      tissue: 11,
      "positive %": 54.5,
      "H-score": 118.2,
    };
    expect(scoreOf(row)).toEqual({
      glass: 10,
      negative: 5,
      weak: 1,
      moderate: 2,
      strong: 3,
      tissue: 11,
      positive: 54.5,
      hScore: 118.2,
    });
    expect(scoreOf(undefined)).toBeUndefined();
  });
});
