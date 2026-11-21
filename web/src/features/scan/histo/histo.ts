// The histo scan (W5.3), a slide scanner's flow: the overview, the tissue
// found in it (P37), its islands proposed as regions, accepted, dropped,
// edited or redrawn, the focus map's points, and the run, which measures
// the map before its first tile and fits a surface through it.
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

import type { OverviewView } from "../overview";
import type { Mask, Plan } from "../plan";

/** The flow's steps, in order. */
export const STEPS = ["overview", "tissue", "regions", "focus", "run"] as const;
export type Step = (typeof STEPS)[number];

/** A region proposed from an island of tissue, or drawn: kept or dropped. */
export interface Proposed {
  rect: Rect;
  kept: boolean;
}

/** What the flow has so far, which going back keeps. */
export interface Flow {
  overview?: OverviewView | undefined;
  regions: readonly Proposed[];
}

/** How far the flow may go: each step needs what the one before it made. */
export function reachable(step: Step, flow: Flow): boolean {
  switch (step) {
    case "overview":
      return true;
    case "tissue":
      return flow.overview !== undefined;
    case "regions":
      return (flow.overview?.tissue.length ?? 0) > 0;
    case "focus":
    case "run":
      return flow.regions.some((one) => one.kept);
  }
}

/** A region's rectangle on the stage from an overview's pixels: columns [from, to), rows [top, bottom). */
function rectOfPixels(
  view: OverviewView,
  from: number,
  to: number,
  top: number,
  bottom: number,
): Rect {
  const p = view.pixel_um;
  return {
    x: view.origin.x + from * p,
    y: view.origin.y + top * p,
    width: (to - from) * p,
    height: (bottom - top) * p,
  };
}

/** A run of tissue's pixels: its row, and its columns `[from, to)`. */
type Run = [row: number, from: number, to: number];

/**
 * The tissue's islands, each the rectangle about its pixels, the largest
 * first: the overview's runs, `[row, from, to)`, joined where they touch,
 * diagonally too.
 */
export function islandsOf(view: OverviewView): Rect[] {
  const runs = view.tissue
    .map(([row, from, to]): Run => [row ?? 0, from ?? 0, to ?? 0])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const parent = runs.map((_, i) => i);
  const find = (i: number): number => {
    while (parent[i] !== i) i = parent[i] = parent[parent[i]!]!;
    return i;
  };
  const byRow = new Map<number, number[]>();
  runs.forEach(([row], i) => byRow.set(row, [...(byRow.get(row) ?? []), i]));
  runs.forEach(([row, from, to], i) => {
    for (const j of byRow.get(row - 1) ?? []) {
      const [, above, until] = runs[j]!;
      if (above <= to && from <= until) parent[find(i)] = find(j);
    }
  });
  const islands = new Map<
    number,
    { from: number; to: number; top: number; bottom: number; pixels: number }
  >();
  runs.forEach(([row, from, to], i) => {
    const root = find(i);
    const now = islands.get(root);
    islands.set(root, {
      from: Math.min(now?.from ?? from, from),
      to: Math.max(now?.to ?? to, to),
      top: Math.min(now?.top ?? row, row),
      bottom: Math.max(now?.bottom ?? row + 1, row + 1),
      pixels: (now?.pixels ?? 0) + (to - from),
    });
  });
  return [...islands.values()]
    .sort((a, b) => b.pixels - a.pixels)
    .map(({ from, to, top, bottom }) => rectOfPixels(view, from, to, top, bottom));
}

/** The regions proposed for an overview's tissue: an island each, all kept. */
export function proposedOf(view: OverviewView): Proposed[] {
  return islandsOf(view).map((rect) => ({ rect, kept: true }));
}

/**
 * The regions kept as one mask for the plan, in the overview's pixels over
 * their bounds: each cell whose middle is in one of them is in.
 */
export function maskOf(view: OverviewView, regions: readonly Proposed[]): Mask | undefined {
  const kept = regions.filter((one) => one.kept).map((one) => one.rect);
  if (kept.length === 0) return undefined;
  const p = view.pixel_um;
  const x0 = Math.min(...kept.map((r) => r.x));
  const y0 = Math.min(...kept.map((r) => r.y));
  const width = Math.ceil((Math.max(...kept.map((r) => r.x + r.width)) - x0) / p);
  const height = Math.ceil((Math.max(...kept.map((r) => r.y + r.height)) - y0) / p);
  const inside = (x: number, y: number) =>
    kept.some((r) => x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height);
  const cells: boolean[] = [];
  for (let row = 0; row < height; row++) {
    for (let column = 0; column < width; column++) {
      cells.push(inside(x0 + (column + 0.5) * p, y0 + (row + 0.5) * p));
    }
  }
  return { origin: { x: x0, y: y0 }, cell_um: p, width, height, cells };
}

/**
 * Where the run will measure focus for its map, as the scan engine chooses
 * them (P37): the tile nearest each middle of a `columns` × `rows` grid
 * over its tiles' centres, each once.
 */
export function mapPointsOf(plan: Plan, columns: number, rows: number): Point[] {
  const xs = plan.tiles.map((t) => t.x_um);
  const ys = plan.tiles.map((t) => t.y_um);
  if (xs.length === 0) return [];
  const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  const points: Point[] = [];
  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < columns; i++) {
      const target = {
        x: x0 + ((i + 0.5) / columns) * (x1 - x0),
        y: y0 + ((j + 0.5) / rows) * (y1 - y0),
      };
      const nearest = plan.tiles.reduce((best, tile) =>
        Math.hypot(tile.x_um - target.x, tile.y_um - target.y) <
        Math.hypot(best.x_um - target.x, best.y_um - target.y)
          ? tile
          : best,
      );
      if (!points.some((p) => p.x === nearest.x_um && p.y === nearest.y_um))
        points.push({ x: nearest.x_um, y: nearest.y_um });
    }
  }
  return points;
}

/** A focus map, fitted: as a scan's plan holds it once measured. */
export type FocusMap = Extract<Plan["z"], { kind: "map" }>["map"];

/** The thin plate's kernel, U(r) = r² ln r, 0 at 0, as the server's. */
function kernel(du: number, dv: number): number {
  const r2 = du * du + dv * dv;
  return r2 > 0 ? (r2 * Math.log(r2)) / 2 : 0;
}

/**
 * The map's surface at `at`, in µm, evaluated as the server evaluates it:
 * from its centre, in mm, a plane's three coefficients, or a thin plate's
 * weight for each used point and then its plane's (Bookstein, 1989).
 */
export function zAt(map: FocusMap, at: Point): number {
  const mm = (x: number, y: number) =>
    [(x - map.centre.x) / 1e3, (y - map.centre.y) / 1e3] as const;
  const [u, v] = mm(at.x, at.y);
  const c = map.coefficients;
  if (map.surface.kind === "plane") return c[0]! + c[1]! * u + c[2]! * v;
  const n = map.used.length;
  const bending = map.used.reduce((sum, p, i) => {
    const [pu, pv] = mm(p.x_um, p.y_um);
    return sum + c[i]! * kernel(u - pu, v - pv);
  }, 0);
  return bending + c[n]! + c[n + 1]! * u + c[n + 2]! * v;
}
