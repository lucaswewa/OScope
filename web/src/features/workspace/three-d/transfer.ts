// A volume's transfer function (W7.8): each sample's opacity, by its value
// from 0 to 1 of the volume's range, through points joined by straight lines,
// and its colour through a colour map. The points are drawn on a chart and
// dragged.

/** A point of the opacity's line: a value, and its opacity, each from 0 to 1. */
export interface Point {
  value: number;
  opacity: number;
}

/** The line at first: nothing below a fifth of the range, rising to a half at its top. */
export const START: readonly Point[] = [
  { value: 0, opacity: 0 },
  { value: 0.2, opacity: 0 },
  { value: 1, opacity: 0.5 },
];

const clamp = (v: number) => Math.min(1, Math.max(0, v));

/** `points` as the line takes them: each within 0 to 1, in order of value. */
export function ordered(points: readonly Point[]): Point[] {
  return points
    .map((p) => ({ value: clamp(p.value), opacity: clamp(p.opacity) }))
    .sort((a, b) => a.value - b.value);
}

/**
 * The line's points over a volume's `range`: each a sample's value and its
 * opacity, in order, for the renderer to join by straight lines.
 */
export function nodesOf(points: readonly Point[], range: [number, number]): [number, number][] {
  const [least, most] = range;
  return ordered(points).map((p) => [least + p.value * (most - least), p.opacity]);
}

/** Point `index` moved to `to`: the ends keep their values, at 0 and 1. */
export function moved(points: readonly Point[], index: number, to: Point): Point[] {
  const line = ordered(points);
  const end = index === 0 || index === line.length - 1;
  const value = end ? line[index]!.value : to.value;
  return ordered(line.map((p, i) => (i === index ? { value, opacity: to.opacity } : p)));
}

/** A volume's samples' range, those that are numbers: the transfer function's 0 to 1. */
export function rangeOf(samples: ArrayLike<number>): [number, number] {
  let [least, most] = [Infinity, -Infinity];
  for (let i = 0; i < samples.length; i++) {
    const v = samples[i]!;
    if (Number.isFinite(v)) [least, most] = [Math.min(least, v), Math.max(most, v)];
  }
  return least <= most ? [least, most === least ? least + 1 : most] : [0, 1];
}
