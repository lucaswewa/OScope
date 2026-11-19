// The overlays' geometry (W3.5, ADR-0049): everything in the sensor's
// pixels, as the view is. A rectangle from two corners, moved and resized
// within the frame; what a key does to a handle; a region on the camera's
// grid; and where a grid's lines fall.
import type { PixelRect } from "@/api/preview/message";

export type Point = [number, number];

/** The rectangle two corners span, in whole pixels, within `extent`. */
export function spanned(a: Point, b: Point, extent: PixelRect): PixelRect {
  const clampX = (x: number) => Math.min(Math.max(x, extent.x), extent.x + extent.width);
  const clampY = (y: number) => Math.min(Math.max(y, extent.y), extent.y + extent.height);
  const [x0, x1] = [clampX(Math.min(a[0], b[0])), clampX(Math.max(a[0], b[0]))];
  const [y0, y1] = [clampY(Math.min(a[1], b[1])), clampY(Math.max(a[1], b[1]))];
  const x = Math.floor(x0);
  const y = Math.floor(y0);
  return { x, y, width: Math.max(1, Math.ceil(x1) - x), height: Math.max(1, Math.ceil(y1) - y) };
}

/** `rect` moved by `by`, kept within `extent`. */
export function moved(rect: PixelRect, by: Point, extent: PixelRect): PixelRect {
  const x = Math.min(Math.max(rect.x + by[0], extent.x), extent.x + extent.width - rect.width);
  const y = Math.min(Math.max(rect.y + by[1], extent.y), extent.y + extent.height - rect.height);
  return { ...rect, x: Math.round(x), y: Math.round(y) };
}

/** A rectangle's corners, by name. */
export const CORNERS = ["topLeft", "topRight", "bottomRight", "bottomLeft"] as const;
export type Corner = (typeof CORNERS)[number];

/** Where a corner of `rect` is. */
export function cornerOf(rect: PixelRect, corner: Corner): Point {
  const right = corner === "topRight" || corner === "bottomRight";
  const bottom = corner === "bottomLeft" || corner === "bottomRight";
  return [rect.x + (right ? rect.width : 0), rect.y + (bottom ? rect.height : 0)];
}

/** `rect` with `corner` taken to `to`, the opposite corner kept: within `extent`. */
export function resized(rect: PixelRect, corner: Corner, to: Point, extent: PixelRect): PixelRect {
  const opposite = CORNERS[(CORNERS.indexOf(corner) + 2) % 4]!;
  return spanned(cornerOf(rect, opposite), to, extent);
}

/** What an arrow key moves a handle by, in the sensor's pixels: ten with Shift. */
export function nudge(key: string, shift: boolean): Point | undefined {
  const step = shift ? 10 : 1;
  const ways: Record<string, Point> = {
    ArrowLeft: [-step, 0],
    ArrowRight: [step, 0],
    ArrowUp: [0, -step],
    ArrowDown: [0, step],
  };
  return Object.hasOwn(ways, key) ? ways[key] : undefined;
}

/**
 * `rect` on the camera's region grid, as the camera takes one: its corners
 * rounded out to the grid, and at least the least size.
 */
export function onGrid(rect: PixelRect, grid: { x: number; y: number; least: number }): PixelRect {
  const x = Math.floor(rect.x / grid.x) * grid.x;
  const y = Math.floor(rect.y / grid.y) * grid.y;
  const right = Math.ceil((rect.x + rect.width) / grid.x) * grid.x;
  const bottom = Math.ceil((rect.y + rect.height) / grid.y) * grid.y;
  return { x, y, width: Math.max(grid.least, right - x), height: Math.max(grid.least, bottom - y) };
}

/** The spacing of a grid's lines in the sensor's pixels: 1, 2 or 5 times a power of ten, some `apart` canvas pixels apart. */
export function gridStep(scale: number, apart = 120): number {
  const most = apart / scale;
  const power = 10 ** Math.floor(Math.log10(most));
  return [5, 2, 1].map((k) => k * power).find((step) => step <= most) ?? power;
}

/** The grid's lines across `extent`, `step` apart, on the sensor's multiples of it. */
export function gridLines(extent: PixelRect, step: number): { xs: number[]; ys: number[] } {
  const along = (from: number, length: number) => {
    const lines: number[] = [];
    for (let at = Math.ceil(from / step) * step; at <= from + length; at += step) lines.push(at);
    return lines;
  };
  return { xs: along(extent.x, extent.width), ys: along(extent.y, extent.height) };
}
