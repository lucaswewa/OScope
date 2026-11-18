// A chart drawn on a canvas (W0.7): its grid, axes and words, the bands, the
// series, and the cursor. Everything is drawn in the page's pixels, scaled to
// the screen's by the caller.
import { place, type Scale, type Tick } from "./scale";

export type SeriesKind = "line" | "area" | "histogram" | "points";

export interface Series {
  name: string;
  kind: SeriesKind;
  /** In order of x. For a histogram, each bin's lower edge. */
  x: ArrayLike<number>;
  y: ArrayLike<number>;
  /** A histogram's bins' width, the same for each. */
  width?: number;
}

/** A band between two lines, drawn faintly in its series' colour. */
export interface Band {
  /** Which series it goes with, by its place. */
  of: number;
  x: ArrayLike<number>;
  lower: ArrayLike<number>;
  upper: ArrayLike<number>;
}

/** What a chart is drawn in: each series' colour, the grid's, the axes', the words' and their font. */
export interface Colours {
  series: string[];
  grid: string;
  axis: string;
  text: string;
  font: string;
}

/** The plot's edges, and the canvas's height, in the page's pixels. */
export interface Frame {
  left: number;
  top: number;
  right: number;
  bottom: number;
  height: number;
}

/** Each series' dash, for lines, and its mark, for points: told apart without colour. */
export const DASHES: readonly number[][] = [[], [6, 4], [2, 3], [8, 3, 2, 3]];
export const MARKS = ["circle", "square", "triangle", "diamond"] as const;

const dash = (n: number): number[] => DASHES[n % DASHES.length]!;
const colour = (colours: Colours, n: number): string => colours.series[n % colours.series.length]!;

/** A crisp line of one pixel lies on a pixel's middle. */
const crisp = (at: number): number => Math.round(at) + 0.5;

/** The grid, the axes and their words, around the plot. */
export function drawAxes(
  pen: CanvasRenderingContext2D,
  frame: Frame,
  x: { scale: Scale; ticks: Tick[]; title: string },
  y: { scale: Scale; ticks: Tick[]; title: string },
  colours: Colours,
): void {
  pen.save();
  pen.lineWidth = 1;
  pen.strokeStyle = colours.grid;
  pen.beginPath();
  for (const tick of x.ticks) {
    const at = crisp(place(x.scale, tick.value));
    pen.moveTo(at, frame.top);
    pen.lineTo(at, frame.bottom);
  }
  for (const tick of y.ticks) {
    const at = crisp(place(y.scale, tick.value));
    pen.moveTo(frame.left, at);
    pen.lineTo(frame.right, at);
  }
  pen.stroke();

  pen.strokeStyle = colours.axis;
  pen.beginPath();
  pen.moveTo(crisp(frame.left), frame.top);
  pen.lineTo(crisp(frame.left), crisp(frame.bottom));
  pen.lineTo(frame.right, crisp(frame.bottom));
  pen.stroke();

  pen.fillStyle = colours.text;
  pen.font = colours.font;
  pen.textAlign = "center";
  pen.textBaseline = "top";
  for (const tick of x.ticks)
    pen.fillText(tick.label, place(x.scale, tick.value), frame.bottom + 6);
  pen.textAlign = "right";
  pen.textBaseline = "middle";
  for (const tick of y.ticks) pen.fillText(tick.label, frame.left - 6, place(y.scale, tick.value));

  pen.textAlign = "center";
  pen.textBaseline = "bottom";
  pen.fillText(x.title, (frame.left + frame.right) / 2, frame.height);
  pen.translate(0, (frame.top + frame.bottom) / 2);
  pen.rotate(-Math.PI / 2);
  pen.textBaseline = "top";
  pen.fillText(y.title, 0, 0);
  pen.restore();
}

/** Traces a line through the points it can place, starting again after a gap. */
function trace(
  pen: CanvasRenderingContext2D,
  xs: ArrayLike<number>,
  ys: ArrayLike<number>,
  x: Scale,
  y: Scale,
): void {
  let drawing = false;
  for (let n = 0; n < xs.length; n++) {
    const px = place(x, xs[n]!);
    const py = place(y, ys[n]!);
    if (!Number.isFinite(px) || !Number.isFinite(py)) {
      drawing = false;
      continue;
    }
    if (drawing) pen.lineTo(px, py);
    else pen.moveTo(px, py);
    drawing = true;
  }
}

/** A band, filled faintly between its two lines. */
export function drawBand(
  pen: CanvasRenderingContext2D,
  band: Band,
  x: Scale,
  y: Scale,
  colours: Colours,
): void {
  pen.save();
  pen.fillStyle = colour(colours, band.of);
  pen.globalAlpha = 0.2;
  pen.beginPath();
  for (let n = 0; n < band.x.length; n++)
    pen.lineTo(place(x, band.x[n]!), place(y, band.upper[n]!));
  for (let n = band.x.length - 1; n >= 0; n--)
    pen.lineTo(place(x, band.x[n]!), place(y, band.lower[n]!));
  pen.closePath();
  pen.fill();
  pen.restore();
}

/**
 * A histogram's outline, as steps. Where bins are narrower than a pixel,
 * each column of pixels takes the highest of those it covers, so that no
 * peak is lost.
 */
function histogram(
  pen: CanvasRenderingContext2D,
  series: Series,
  x: Scale,
  y: Scale,
  base: number,
): void {
  const width = series.width ?? 1;
  const [left, right] = [Math.min(x.from, x.to), Math.max(x.from, x.to)];
  let column = Number.NaN;
  let top = 0;
  let started = false;
  let end = left;
  const step = (from: number, to: number, at: number): void => {
    if (!started) pen.moveTo(from, base);
    started = true;
    pen.lineTo(from, at);
    pen.lineTo(to, at);
    end = to;
  };
  for (let n = 0; n < series.y.length; n++) {
    const from = place(x, series.x[n]!);
    const to = place(x, series.x[n]! + width);
    if (to <= left || from >= right) continue;
    const at = Math.min(base, place(y, series.y[n]!));
    if (to - from >= 1) {
      if (!Number.isNaN(column)) step(column, column + 1, top);
      column = Number.NaN;
      step(Math.max(from, left), Math.min(to, right), at);
    } else if (Math.floor(from) === column) {
      top = Math.min(top, at);
    } else {
      if (!Number.isNaN(column)) step(column, column + 1, top);
      column = Math.floor(from);
      top = at;
    }
  }
  if (!Number.isNaN(column)) step(column, column + 1, top);
  if (started) pen.lineTo(end, base);
}

/** A mark of a point, in its series' shape. */
function mark(
  pen: CanvasRenderingContext2D,
  shape: (typeof MARKS)[number],
  px: number,
  py: number,
  size: number,
): void {
  pen.beginPath();
  if (shape === "circle") pen.arc(px, py, size, 0, 2 * Math.PI);
  else if (shape === "square") pen.rect(px - size, py - size, 2 * size, 2 * size);
  else if (shape === "triangle") {
    pen.moveTo(px, py - size * 1.2);
    pen.lineTo(px + size * 1.1, py + size * 0.8);
    pen.lineTo(px - size * 1.1, py + size * 0.8);
    pen.closePath();
  } else {
    pen.moveTo(px, py - size * 1.3);
    pen.lineTo(px + size * 1.3, py);
    pen.lineTo(px, py + size * 1.3);
    pen.lineTo(px - size * 1.3, py);
    pen.closePath();
  }
  pen.fill();
}

/** A series, the `n`th, in its kind, colour and dash or mark. */
export function drawSeries(
  pen: CanvasRenderingContext2D,
  series: Series,
  n: number,
  x: Scale,
  y: Scale,
  colours: Colours,
): void {
  pen.save();
  pen.strokeStyle = pen.fillStyle = colour(colours, n);
  pen.lineWidth = 1.5;
  pen.lineJoin = "round";
  const base = Math.max(y.from, y.to);
  if (series.kind === "points") {
    const shape = MARKS[n % MARKS.length]!;
    for (let k = 0; k < series.x.length; k++) {
      const px = place(x, series.x[k]!);
      const py = place(y, series.y[k]!);
      if (Number.isFinite(px) && Number.isFinite(py)) mark(pen, shape, px, py, 3.5);
    }
  } else if (series.kind === "histogram") {
    pen.beginPath();
    histogram(pen, series, x, y, base);
    pen.globalAlpha = 0.3;
    pen.fill();
    pen.globalAlpha = 1;
    pen.stroke();
  } else {
    pen.setLineDash(dash(n));
    pen.beginPath();
    trace(pen, series.x, series.y, x, y);
    pen.stroke();
    if (series.kind === "area" && series.x.length > 0) {
      pen.lineTo(place(x, series.x[series.x.length - 1]!), base);
      pen.lineTo(place(x, series.x[0]!), base);
      pen.closePath();
      pen.globalAlpha = 0.25;
      pen.fill();
    }
  }
  pen.restore();
}

/** The cursor: a line at its x, and a ring on each series' point there. */
export function drawCursor(
  pen: CanvasRenderingContext2D,
  frame: Frame,
  at: number,
  points: { x: number; y: number; n: number }[],
  colours: Colours,
): void {
  pen.save();
  pen.strokeStyle = colours.text;
  pen.lineWidth = 1;
  pen.setLineDash([3, 3]);
  pen.beginPath();
  pen.moveTo(crisp(at), frame.top);
  pen.lineTo(crisp(at), frame.bottom);
  pen.stroke();
  pen.setLineDash([]);
  pen.lineWidth = 2;
  for (const point of points) {
    if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) continue;
    pen.strokeStyle = colour(colours, point.n);
    pen.beginPath();
    pen.arc(point.x, point.y, 5, 0, 2 * Math.PI);
    pen.stroke();
  }
  pen.restore();
}
