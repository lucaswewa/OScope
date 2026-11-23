// An image to count, and a recipe that counts (W7.4): bright discs of six
// sizes on a dark ground, and a recipe that finds them, splits and measures
// them; a project made with the image imported, and the recipe kept.
import { Server as Api } from "../../src/api/client.ts";
import { saveRecipe, type Recipe } from "../../src/api/pipeline.ts";
import { imported, made } from "./records.ts";
import { rgbTiffOf, tiffOf } from "./tiff.ts";

/** Each disc's cell, in pixels: its radius is 4 to 14, by its place. */
const CELL = 48;

/** 8 by 6 discs: of radius 4 + 2k, k from 0 to 5, each a sixth of them. */
export const discs = (): Buffer =>
  tiffOf(8 * CELL, 6 * CELL, 1, (x, y) => {
    const [i, j] = [Math.floor(x / CELL), Math.floor(y / CELL)];
    const radius = 4 + 2 * ((i + j) % 6);
    const [dx, dy] = [(x % CELL) - CELL / 2, (y % CELL) - CELL / 2];
    return dx * dx + dy * dy <= radius * radius ? 50_000 : 2_000;
  });

/** The discs counted: those of `least` µm² or more, a pixel being a µm. */
export function counting(least: number): Recipe {
  return {
    format: 1,
    name: "Discs counted",
    inputs: [{ name: "image", type: "image" }],
    nodes: [
      { id: "threshold", node: "threshold.otsu", params: {} },
      { id: "split", node: "cells.split", params: {} },
      { id: "measure", node: "cells.measure", params: { min_area: least, max_area: 10_000 } },
    ],
    edges: [
      { from: "input.image", to: "threshold.image" },
      { from: "threshold.mask", to: "split.mask" },
      { from: "split.labels", to: "measure.labels" },
      { from: "input.image", to: "measure.image" },
    ],
    outputs: [{ name: "objects", from: "measure.objects" }],
  };
}

/** The discs counted, as `counting` does, and their outlines kept too (W7.6). */
export function outlining(least: number): Recipe {
  const recipe = counting(least);
  return {
    ...recipe,
    name: "Discs outlined",
    outputs: [...recipe.outputs, { name: "outlines", from: "measure.outlines" }],
  };
}

/** A slide stained with hematoxylin and DAB: blue discs and brown ones, on glass (W7.6). */
export const stained = (): Buffer =>
  rgbTiffOf(8 * CELL, 6 * CELL, (x, y) => {
    const [i, j] = [Math.floor(x / CELL), Math.floor(y / CELL)];
    const [dx, dy] = [(x % CELL) - CELL / 2, (y % CELL) - CELL / 2];
    if (dx * dx + dy * dy > 16 * 16) return [235, 235, 235];
    return (i + j) % 2 === 0 ? [70, 80, 160] : [130, 80, 40];
  });

/** A project, the discs imported into it, and the recipe kept as `count-discs`. */
export async function prepared(url: string): Promise<{ project: string; acquisition: string }> {
  const project = await made(url, "Discs");
  const acquisition = await imported(url, project, "discs.tif", discs());
  await saveRecipe("count-discs", counting(10), { on: new Api(url) });
  return { project, acquisition };
}
