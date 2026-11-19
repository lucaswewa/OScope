// The live viewer's arithmetic (W3.1, ADR-0049), on W1.5's view: the
// pointer's place on the canvas, the zoom's steps, the sensor's pixel under
// the pointer and its distance from the sensor's centre at the sample, and
// the scale bar's round lengths. The canvas's sizes are in device pixels;
// a view's scale is the device pixels a sensor pixel is drawn across.
import type { PixelRect } from "@/api/preview/message";
import { sensorAt, zoomed, type Size, type View } from "@/api/preview/view";

/** How much a key, a button or a notch of the wheel zooms. */
export const STEP = 1.25;

/** The wheel's travel that is one notch, in pixels. */
const NOTCH = 100;

/** A point in the canvas's box, in CSS pixels, in the canvas's device pixels. */
export function onCanvas(at: [number, number], box: Size, canvas: Size): [number, number] {
  return [(at[0] * canvas.width) / box.width, (at[1] * canvas.height) / box.height];
}

/** How much the wheel's turn zooms: a notch towards the user zooms in a step. */
export function wheelFactor(deltaY: number): number {
  const notches = Math.max(-4, Math.min(4, -deltaY / NOTCH));
  return STEP ** notches;
}

/** The view showing each sensor pixel on one device pixel, the point under `at` kept. */
export function oneToOne(view: View, at: [number, number], extent: PixelRect, canvas: Size): View {
  return zoomed(view, 1 / view.scale, at, extent, canvas);
}

/** The zoom, as a percentage: 100 is a sensor pixel on a device pixel. */
export function percent(view: View): number {
  return view.scale * 100;
}

/** The sensor's pixel under the canvas's point `at`: none outside the frame. */
export function pixelAt(
  view: View,
  at: [number, number],
  canvas: Size,
  extent: PixelRect,
): [number, number] | undefined {
  const [x, y] = sensorAt(view, at, canvas);
  const inside =
    x >= extent.x && y >= extent.y && x < extent.x + extent.width && y < extent.y + extent.height;
  return inside ? [Math.floor(x), Math.floor(y)] : undefined;
}

/**
 * A sensor pixel's centre, from the sensor's centre, in µm at the sample,
 * along the sensor's axes: `side` is a sensor pixel's side at the sample.
 * The stage's axes are the calibration's to give (W3.3).
 */
export function fromCentre(pixel: [number, number], sensor: Size, side: number): [number, number] {
  return [(pixel[0] + 0.5 - sensor.width / 2) * side, (pixel[1] + 0.5 - sensor.height / 2) * side];
}

/** The longest length of 1, 2 or 5 times a power of ten, in µm, that is at most `most`. */
export function roundLength(most: number): number {
  const power = 10 ** Math.floor(Math.log10(most));
  return [5, 2, 1].map((k) => k * power).find((length) => length <= most * (1 + 1e-9)) ?? power;
}

/** A scale bar: its length at the sample, in µm, and on the screen, in CSS pixels. */
export interface Bar {
  length: number;
  width: number;
}

/**
 * The scale bar for a sensor pixel's `side` at the sample, in µm, on the
 * `view`, at most `room` CSS pixels long: none without the side.
 */
export function scaleBar(
  side: number | null | undefined,
  view: View,
  ratio: number,
  room: number,
): Bar | undefined {
  if (!side || side <= 0 || room <= 0) return undefined;
  // µm at the sample a CSS pixel spans.
  const perPixel = (side * ratio) / view.scale;
  const length = roundLength(room * perPixel);
  return { length, width: length / perPixel };
}
