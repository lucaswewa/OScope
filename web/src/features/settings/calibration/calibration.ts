// The calibration view's arithmetic (W8.2): the sets by the objective each
// is for; a set's image as a map's pixels; and the geometry's residuals as
// a chart's points, with how far they reach and their root mean square.
import type { SetImage } from "@/api/calibration";
import type { Schema } from "@/api/client";
import type { Series } from "@/ui/charts/draw";

export type Summary = Schema<"calibration_sets_value">[number];

/**
 * The objective a set is for, from its key: a geometry's and a flat
 * field's; none for the dark frames', the colours' and the stage's, which
 * are the instrument's.
 */
export function objectiveOf(key: string): string | undefined {
  const [kind, objective] = key.split("/");
  return kind === "geometry" || kind === "flat" ? objective : undefined;
}

/** Sets of one objective, or the instrument's. */
export interface Group {
  objective: string | undefined;
  sets: Summary[];
}

/** The sets by objective, in the objectives' order, then the instrument's: each as listed. */
export function byObjective(sets: readonly Summary[]): Group[] {
  const groups = new Map<string | undefined, Summary[]>();
  for (const set of sets) {
    const objective = objectiveOf(set.key);
    groups.set(objective, [...(groups.get(objective) ?? []), set]);
  }
  const names = [...groups.keys()]
    .filter((name) => name !== undefined)
    .sort((a, b) => a.localeCompare(b, "en", { numeric: true }));
  const order = groups.has(undefined) ? [...names, undefined] : names;
  return order.map((objective) => ({ objective, sets: groups.get(objective)! }));
}

/** A set's image as a map's pixels, and the values its colours run between. */
export interface Map {
  rgba: Uint8ClampedArray<ArrayBuffer>;
  low: number;
  high: number;
}

/**
 * `image` as a map: each block, its channels' mean, from the lowest of
 * them to the highest through `table`'s 256 colours, three bytes each.
 */
export function mapOf(image: SetImage, table: Uint8Array): Map {
  const { width, height, channels, values } = image;
  const levels = Array.from({ length: width * height }, (_, i) => {
    let sum = 0;
    for (let c = 0; c < channels; c++) sum += values[i * channels + c]!;
    return sum / channels;
  });
  const low = levels.length > 0 ? Math.min(...levels) : 0;
  const high = levels.length > 0 ? Math.max(...levels) : 0;
  const span = high > low ? high - low : 1;
  const rgba = new Uint8ClampedArray(levels.length * 4);
  levels.forEach((level, i) => {
    const at = Math.round(((level - low) / span) * 255) * 3;
    rgba.set([table[at]!, table[at + 1]!, table[at + 2]!, 255], i * 4);
  });
  return { rgba, low, high };
}

/** The residuals' chart: its points, and how far from 0 its y axis reaches. */
export interface Residuals {
  series: Series[];
  reach: number;
  /** Their root mean square, in pixels, as the fit's error is. */
  rms: number;
}

/** Each move's residual, x against y, in pixels, as points named `name`, in order of x. */
export function residualsOf(residuals: readonly (readonly number[])[], name: string): Residuals {
  const sorted = [...residuals].sort((a, b) => (a[0] ?? 0) - (b[0] ?? 0));
  const x = sorted.map((r) => r[0] ?? 0);
  const y = sorted.map((r) => r[1] ?? 0);
  const squares = x.reduce((sum, dx, i) => sum + dx * dx + y[i]! * y[i]!, 0);
  const rms = residuals.length > 0 ? Math.sqrt(squares / residuals.length) : 0;
  // About 0, with a margin, the furthest either way: how far off the fit is reads at a glance.
  const most = Math.max(0, ...x.map(Math.abs), ...y.map(Math.abs));
  const reach = most > 0 ? most * 1.25 : 0.1;
  return { series: [{ name, kind: "points", x, y }], reach, rms };
}

/**
 * Whether a set can be exported: not a dark frame or a flat field, whose
 * images stay on the instrument they were taken on (P33).
 */
export function exportable(set: Pick<Summary, "kind">): boolean {
  return set.kind !== "dark" && set.kind !== "flat";
}

/** The file a set is exported to, by its key and version. */
export function fileOf(set: Pick<Summary, "key" | "version">): string {
  const key = set.key.replace(/[^A-Za-z0-9.+]+/g, "-");
  return `oscope-calibration-${key}-v${set.version}.json`;
}
