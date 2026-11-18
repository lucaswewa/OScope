// A chart's axes (W0.7): where a value goes along one, the round numbers it
// is marked at, how each is written, and how a view of it zooms and pans.
// The ticks are Heckbert's "nice numbers" (Graphics Gems, 1990).

export type ScaleKind = "linear" | "log";

/** An axis's values from `min` to `max`, drawn from pixel `from` to pixel `to`. */
export interface Scale {
  kind: ScaleKind;
  min: number;
  max: number;
  from: number;
  to: number;
}

export interface Tick {
  value: number;
  label: string;
}

/**
 * A nice number near `x`: 1, 2, 5 or 10 times a power of ten. Rounded, the
 * nearest such; otherwise the least that isn't below `x`.
 */
export function nice(x: number, round: boolean): number {
  const power = 10 ** Math.floor(Math.log10(x));
  const f = x / power;
  const n = round
    ? f < 1.5
      ? 1
      : f < 3
        ? 2
        : f < 7
          ? 5
          : 10
    : f <= 1
      ? 1
      : f <= 2
        ? 2
        : f <= 5
          ? 5
          : 10;
  return n * power;
}

/** A range a chart can show: one of nothing is widened by one each way, or by a tenth of its value. */
export function widened(min: number, max: number): [number, number] {
  if (max > min) return [min, max];
  const by = Math.max(1, Math.abs(min) / 10);
  return [min - by, max + by];
}

/** The step between the ticks of a linear axis, for about `wanted` of them over its range. */
export function stepOf(min: number, max: number, wanted: number): number {
  const [low, high] = widened(min, max);
  return nice(nice(high - low, false) / Math.max(1, wanted - 1), true);
}

/** Decimals a step's multiples need. */
const digitsOf = (step: number): number => Math.max(0, -Math.floor(Math.log10(step) + 1e-9));

/** A multiple of a step, without the float's noise: 0.6, not 0.6000000000000001. */
const multiple = (n: number, step: number): number =>
  Number((n * step).toFixed(Math.min(20, digitsOf(step) + 1)));

/** A range widened out to a tick at each end: Heckbert's loose labels. */
export function loose(min: number, max: number, wanted: number): [number, number] {
  const [low, high] = widened(min, max);
  const step = stepOf(low, high, wanted);
  return [multiple(Math.floor(low / step), step), multiple(Math.ceil(high / step), step)];
}

/**
 * A value as an axis writes it: with `digits` decimals and a point; from
 * 10⁶ up, as 2.5e6. Never "-0".
 */
export function written(value: number, digits: number): string {
  if (Math.abs(value) >= 1e6) {
    return Number(value.toPrecision(12)).toExponential().replace("e+", "e");
  }
  const text = value.toFixed(digits);
  return /^-0(\.0*)?$/.test(text) ? text.slice(1) : text;
}

/** The ticks of a linear axis between `min` and `max`, about `wanted` of them. */
export function linearTicks(min: number, max: number, wanted: number): Tick[] {
  const [low, high] = widened(min, max);
  const step = stepOf(low, high, wanted);
  const digits = digitsOf(step);
  const ticks: Tick[] = [];
  for (let n = Math.ceil(low / step - 1e-9); n <= Math.floor(high / step + 1e-9); n++) {
    const value = multiple(n, step);
    ticks.push({ value, label: written(value, digits) });
  }
  return ticks;
}

const SUPERSCRIPT = "⁰¹²³⁴⁵⁶⁷⁸⁹";
/** Ten to a power, as an axis writes it: 0.001 to 1000 plainly, and 10⁴ beyond. */
function power(exponent: number, times = 1): string {
  if (exponent >= -3 && exponent <= 3)
    return written(times * 10 ** exponent, Math.max(0, -exponent));
  const raised = [...String(Math.abs(exponent))]
    .map((digit) => SUPERSCRIPT[Number(digit)])
    .join("");
  return `${times === 1 ? "" : `${times}×`}10${exponent < 0 ? "⁻" : ""}${raised}`;
}

/**
 * The ticks of a logarithmic axis between `min` and `max`, both above 0: at
 * powers of ten; with fewer than three decades, at 2 and 5 times them too;
 * with more than ten, at every second, fifth or tenth.
 */
export function logTicks(min: number, max: number): Tick[] {
  const low = Math.log10(min);
  const high = Math.log10(max);
  const decades = high - low;
  const every = decades > 50 ? 10 : decades > 25 ? 5 : decades > 10 ? 2 : 1;
  const ticks: Tick[] = [];
  for (let exponent = Math.floor(low); exponent <= Math.ceil(high); exponent++) {
    const times = decades < 3 ? [1, 2, 5] : exponent % every === 0 ? [1] : [];
    for (const n of times) {
      const value = n * 10 ** exponent;
      if (value >= min * (1 - 1e-9) && value <= max * (1 + 1e-9)) {
        ticks.push({ value, label: power(exponent, n) });
      }
    }
  }
  return ticks;
}

/** The ticks of a scale, about one for each `spacing` pixels along it, and at least two. */
export function ticksOf(scale: Scale, spacing: number): Tick[] {
  if (scale.kind === "log") return logTicks(scale.min, scale.max);
  const wanted = Math.max(2, Math.floor(Math.abs(scale.to - scale.from) / spacing));
  return linearTicks(scale.min, scale.max, wanted);
}

/** Where a value goes, in pixels; nowhere for a value a log scale can't show. */
export function place(scale: Scale, value: number): number {
  const at =
    scale.kind === "log"
      ? value > 0
        ? (Math.log10(value) - Math.log10(scale.min)) /
          (Math.log10(scale.max) - Math.log10(scale.min))
        : Number.NaN
      : (value - scale.min) / (scale.max - scale.min);
  return scale.from + at * (scale.to - scale.from);
}

/** The value at a pixel. */
export function valueAt(scale: Scale, pixel: number): number {
  const at = (pixel - scale.from) / (scale.to - scale.from);
  if (scale.kind === "log") {
    const [low, high] = [Math.log10(scale.min), Math.log10(scale.max)];
    return 10 ** (low + at * (high - low));
  }
  return scale.min + at * (scale.max - scale.min);
}

/** The place of the value nearest `x` among `xs`, which are in order: a binary search. */
export function nearestByX(xs: ArrayLike<number>, x: number): number {
  if (xs.length === 0) return -1;
  let low = 0;
  let high = xs.length - 1;
  while (high - low > 1) {
    const middle = (low + high) >> 1;
    if (xs[middle]! <= x) low = middle;
    else high = middle;
  }
  return Math.abs(xs[high]! - x) < Math.abs(xs[low]! - x) ? high : low;
}

/** The place of the point nearest a pixel, by the points' own pixels; -1 if none is drawn. */
export function nearestInPlane(
  xs: ArrayLike<number>,
  ys: ArrayLike<number>,
  x: number,
  y: number,
): number {
  let best = -1;
  let least = Number.POSITIVE_INFINITY;
  for (let n = 0; n < xs.length; n++) {
    const distance = (xs[n]! - x) ** 2 + (ys[n]! - y) ** 2;
    if (distance < least) {
      least = distance;
      best = n;
    }
  }
  return best;
}

/** What part of the data's x a chart shows. */
export interface View {
  min: number;
  max: number;
}

/**
 * A view kept inside the data's, `all`, and no narrower than `least`: a view
 * moved past an end is moved back, keeping its width.
 */
export function kept(view: View, all: View, least: number): View {
  const width = Math.min(all.max - all.min, Math.max(least, view.max - view.min));
  const min = Math.min(Math.max(view.min, all.min), all.max - width);
  return { min, max: min + width };
}

/** A view zoomed in by `factor` (out, below 1) about the value `about`, which stays where it is. */
export function zoomed(view: View, factor: number, about: number, all: View, least: number): View {
  const share = (about - view.min) / (view.max - view.min);
  const width = Math.min(all.max - all.min, Math.max(least, (view.max - view.min) / factor));
  return kept({ min: about - share * width, max: about - share * width + width }, all, least);
}

/** A view moved by `by` along x. */
export function panned(view: View, by: number, all: View, least: number): View {
  return kept({ min: view.min + by, max: view.max + by }, all, least);
}
