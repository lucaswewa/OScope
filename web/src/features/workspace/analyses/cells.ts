// A cell count's result (W7.6, P46): its table of cells, their outlines
// placed on the image they were counted on, only those a view needs, the
// histograms of their measures, and their count per mm².
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

/** A table's rows from its CSV, each a record by the header's names: numbers where they read as numbers. */
export function rowsOf(csv: string): Record<string, string | number>[] {
  const [head, ...lines] = csv.trim().split(/\r?\n/);
  const names = (head ?? "").split(",");
  return lines
    .filter((line) => line !== "")
    .map((line) => {
      const values = line.split(",");
      const read = (value: string) =>
        value !== "" && Number.isFinite(Number(value)) ? Number(value) : value;
      return Object.fromEntries(names.map((name, i) => [name, read(values[i] ?? "")]));
    });
}

/** A cell's outline on the image: its label, its ring, and its bounds, in the image's µm. */
export interface Outline {
  label: number;
  ring: Point[];
  bounds: Rect;
}

/** Where outlines were measured: the recipe's pixel, in µm, and the region read, from the image's corner, in its pixels. */
export interface Measured {
  pixelUm: [number, number];
  /** The region's corner, in the image's pixels. */
  at: [number, number];
  /** The image's own pixel, in µm. */
  imageUm: number;
}

/** `cells.measure`'s outlines, a FeatureCollection in its µm, placed in the image's µm. */
export function outlinesOf(geojson: unknown, measured: Measured): Outline[] {
  const features = (geojson as { features?: unknown[] } | null)?.features ?? [];
  const { pixelUm, at, imageUm } = measured;
  return features.flatMap((feature) => {
    const { geometry, properties } = feature as {
      geometry?: { type?: string; coordinates?: number[][][] };
      properties?: { label?: number };
    };
    const ring = geometry?.type === "Polygon" ? geometry.coordinates?.[0] : undefined;
    if (!ring?.length) return [];
    const points = ring.map(([x = 0, y = 0]) => ({
      x: (x / pixelUm[0] + at[0]) * imageUm,
      y: (y / pixelUm[1] + at[1]) * imageUm,
    }));
    const [xs, ys] = [points.map((p) => p.x), points.map((p) => p.y)];
    const [x, y] = [Math.min(...xs), Math.min(...ys)];
    const bounds = { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y };
    return [{ label: properties?.label ?? 0, ring: points, bounds }];
  });
}

/** Outlines by a grid of `cell` µm squares, for a view's to be found without reading them all. */
export interface Grid {
  cell: number;
  cells: Map<string, number[]>;
  outlines: readonly Outline[];
}

const span = (from: number, size: number, cell: number) => [
  Math.floor(from / cell),
  Math.floor((from + size) / cell),
];

export function gridOf(outlines: readonly Outline[], cell: number): Grid {
  const cells = new Map<string, number[]>();
  outlines.forEach((outline, i) => {
    const [x0, x1] = span(outline.bounds.x, outline.bounds.width, cell);
    const [y0, y1] = span(outline.bounds.y, outline.bounds.height, cell);
    for (let gx = x0!; gx <= x1!; gx++)
      for (let gy = y0!; gy <= y1!; gy++) {
        const key = `${gx},${gy}`;
        cells.set(key, [...(cells.get(key) ?? []), i]);
      }
  });
  return { cell, cells, outlines };
}

/**
 * The outlines a view needs: those whose bounds meet it, each once, in their
 * order. A view wider than the grid's cells are many is met by testing each
 * outline: no wider loop than the outlines.
 */
export function visible(grid: Grid, view: Rect): Outline[] {
  const [x0, x1] = span(view.x, view.width, grid.cell);
  const [y0, y1] = span(view.y, view.height, grid.cell);
  const spanned = (x1! - x0! + 1) * (y1! - y0! + 1);
  const found = new Set<number>();
  if (!(spanned <= grid.cells.size)) grid.outlines.forEach((_, i) => found.add(i));
  else
    for (let gx = x0!; gx <= x1!; gx++)
      for (let gy = y0!; gy <= y1!; gy++)
        for (const i of grid.cells.get(`${gx},${gy}`) ?? []) found.add(i);
  const meets = (b: Rect) =>
    b.x <= view.x + view.width &&
    b.x + b.width >= view.x &&
    b.y <= view.y + view.height &&
    b.y + b.height >= view.y;
  return [...found]
    .sort((a, b) => a - b)
    .map((i) => grid.outlines[i]!)
    .filter((o) => meets(o.bounds));
}

/** A histogram of `values` in `bins` bins from their least to their greatest: each bin's lower edge, its count, and their width. */
export function histogramOf(
  values: readonly number[],
  bins: number,
): { edges: number[]; counts: number[]; width: number } {
  if (!values.length) return { edges: [], counts: [], width: 1 };
  const [least, most] = [Math.min(...values), Math.max(...values)];
  const width = most > least ? (most - least) / bins : 1;
  const counts = new Array<number>(bins).fill(0);
  // The greatest falls in the last bin, not one past it.
  for (const value of values) counts[Math.min(bins - 1, Math.floor((value - least) / width))]!++;
  return { edges: counts.map((_, i) => least + i * width), counts, width };
}

/** Cells per mm², of `count` cells over an area of `um2` µm². */
export const perMm2 = (count: number, um2: number): number => (um2 > 0 ? (count * 1e6) / um2 : 0);
