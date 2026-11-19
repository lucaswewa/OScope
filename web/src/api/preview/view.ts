// The view onto the frame (W1.5): where the preview is drawn, as a
// transform, and the region of the sensor to ask for once zoomed in. The
// frame's extent is the sensor's pixels a whole frame shows, as its header
// says; sizes on the canvas are in device pixels.
import type { PixelRect } from "./message.ts";

/** A width and a height. */
export interface Size {
  width: number;
  height: number;
}

/** What the canvas shows: the sensor's point at its centre, and its pixels a sensor pixel. */
export interface View {
  centre: [number, number];
  scale: number;
}

/** The most canvas pixels a sensor pixel is drawn across. */
export const MOST_SCALE = 16;

/** Within this of a whole pixel is that pixel: what floating point adds isn't asked for. */
const NEAR = 1e-6;

/** The scale at which the whole frame fits the canvas. */
export function fitScale(extent: PixelRect, canvas: Size): number {
  return Math.min(canvas.width / extent.width, canvas.height / extent.height);
}

/** The whole frame, fitted to the canvas and centred. */
export function fitted(extent: PixelRect, canvas: Size): View {
  return {
    centre: [extent.x + extent.width / 2, extent.y + extent.height / 2],
    scale: fitScale(extent, canvas),
  };
}

/**
 * `view`, kept to the frame: never smaller than fitted, nor larger than
 * `MOST_SCALE`, and no edge of the frame inside the canvas unless the whole
 * of that side fits.
 */
export function kept(view: View, extent: PixelRect, canvas: Size): View {
  const scale = Math.min(Math.max(view.scale, fitScale(extent, canvas)), MOST_SCALE);
  const axis = (centre: number, start: number, length: number, room: number) => {
    const half = room / scale / 2;
    if (half * 2 >= length - NEAR) return start + length / 2;
    return Math.min(Math.max(centre, start + half), start + length - half);
  };
  return {
    centre: [
      axis(view.centre[0], extent.x, extent.width, canvas.width),
      axis(view.centre[1], extent.y, extent.height, canvas.height),
    ],
    scale,
  };
}

/** The sensor's point under the canvas's point `at`. */
export function sensorAt(view: View, at: [number, number], canvas: Size): [number, number] {
  return [
    view.centre[0] + (at[0] - canvas.width / 2) / view.scale,
    view.centre[1] + (at[1] - canvas.height / 2) / view.scale,
  ];
}

/** `view` zoomed by `factor` about the canvas's point `at`: the sensor's point under it stays. */
export function zoomed(
  view: View,
  factor: number,
  at: [number, number],
  extent: PixelRect,
  canvas: Size,
): View {
  const [x, y] = sensorAt(view, at, canvas);
  const scale = Math.min(Math.max(view.scale * factor, fitScale(extent, canvas)), MOST_SCALE);
  const centre: [number, number] = [
    x - (at[0] - canvas.width / 2) / scale,
    y - (at[1] - canvas.height / 2) / scale,
  ];
  return kept({ centre, scale }, extent, canvas);
}

/** `view` moved by `by`, in canvas pixels: the image follows the pointer. */
export function panned(view: View, by: [number, number], extent: PixelRect, canvas: Size): View {
  const centre: [number, number] = [
    view.centre[0] - by[0] / view.scale,
    view.centre[1] - by[1] / view.scale,
  ];
  return kept({ centre, scale: view.scale }, extent, canvas);
}

/** Where the sensor's `rect` is drawn on the canvas. */
export function placed(view: View, rect: PixelRect, canvas: Size): PixelRect {
  return {
    x: canvas.width / 2 + (rect.x - view.centre[0]) * view.scale,
    y: canvas.height / 2 + (rect.y - view.centre[1]) * view.scale,
    width: rect.width * view.scale,
    height: rect.height * view.scale,
  };
}

/**
 * The region to ask the server for: none while the whole frame shows, for
 * the whole frame at the viewport's size; once zoomed in, the frame's pixels
 * the canvas shows, in whole pixels, which the server sends at up to their
 * own resolution.
 */
export function roiFor(view: View, extent: PixelRect, canvas: Size): PixelRect | undefined {
  const [left, top] = sensorAt(view, [0, 0], canvas);
  const [right, bottom] = sensorAt(view, [canvas.width, canvas.height], canvas);
  const [down, up] = [(n: number) => Math.floor(n + NEAR), (n: number) => Math.ceil(n - NEAR)];
  const x = Math.max(extent.x, down(left));
  const y = Math.max(extent.y, down(top));
  const width = Math.min(extent.x + extent.width, up(right)) - x;
  const height = Math.min(extent.y + extent.height, up(bottom)) - y;
  if (width >= extent.width && height >= extent.height) return undefined;
  return { x, y, width, height };
}
