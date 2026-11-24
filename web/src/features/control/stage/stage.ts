// The stage's arithmetic (W3.3): lengths in the display's unit, the limits'
// check, the keys' map, and pointing, from the sensor's pixels to the
// stage's µm through the active calibration's matrix.

export type Axis = "x" | "y" | "z";
export const AXES: readonly Axis[] = ["x", "y", "z"];
export type Xyz = Record<Axis, number>;
export type Limits = Record<Axis, { min: number; max: number }>;

/** The display's unit of lengths and positions. */
export type Unit = "um" | "mm";

/** How many µm one of each unit is. */
export const PER: Readonly<Record<Unit, number>> = { um: 1, mm: 1000 };

/** A length in µm, written in `unit`, to a tenth of a µm. */
export function written(um: number, unit: Unit): string {
  return (um / PER[unit]).toFixed(unit === "mm" ? 4 : 1);
}

/**
 * The axes along which `target` is past the limits, of the `axes` checked:
 * homing leaves the stage a micrometre past them, which an axis that
 * doesn't move mustn't be refused for.
 */
export function beyond(target: Xyz, limits: Limits, axes: readonly Axis[] = AXES): Axis[] {
  return axes.filter((axis) => target[axis] < limits[axis].min || target[axis] > limits[axis].max);
}

/** `value` brought within `travel`. */
export function within(value: number, travel: { min: number; max: number }): number {
  return Math.min(travel.max, Math.max(travel.min, value));
}

/** Where a move by `by` from `from` ends. */
export function plus(from: Xyz, by: Partial<Xyz>): Xyz {
  return { x: from.x + (by.x ?? 0), y: from.y + (by.y ?? 0), z: from.z + (by.z ?? 0) };
}

/** The axes a move by `by` moves along. */
export function movedBy(by: Partial<Xyz>): Axis[] {
  return AXES.filter((axis) => (by[axis] ?? 0) !== 0);
}

/** A step along an axis: towards its maximum, or its minimum. */
export interface Step {
  axis: Axis;
  towardsMax: boolean;
}

/**
 * The keys on the focused image: the arrows along x and y, Page Up and Page
 * Down along z. Right and Down go towards the maximum, as the image's own
 * x and y grow: the stage then brings in what lay that way.
 */
const KEYS: Readonly<Record<string, Step>> = {
  ArrowRight: { axis: "x", towardsMax: true },
  ArrowLeft: { axis: "x", towardsMax: false },
  ArrowDown: { axis: "y", towardsMax: true },
  ArrowUp: { axis: "y", towardsMax: false },
  PageUp: { axis: "z", towardsMax: true },
  PageDown: { axis: "z", towardsMax: false },
};

/** The step a key takes, if it takes one. */
export function keyStep(key: string): Step | undefined {
  return Object.hasOwn(KEYS, key) ? KEYS[key] : undefined;
}

/** A step's move, `size` µm along its axis. */
export function stepBy(step: Step, size: number): Partial<Xyz> {
  return { [step.axis]: step.towardsMax ? size : -size };
}

/** A 2 × 2 matrix, by rows. */
export type Matrix = [[number, number], [number, number]];

/** The active geometry calibration's values, as far as pointing needs them. */
export interface Geometry {
  objective: string;
  binning: number;
  /** µm per pixel of the image at `binning` (P31): the move M · d brings what is d pixels from the centre to it. */
  matrix: Matrix;
  /** The fit's error, in the image's pixels. */
  residual_px: number;
}

const isPair = (row: unknown): row is [number, number] =>
  Array.isArray(row) && row.length === 2 && row.every(Number.isFinite);

/** The geometry in a set's values: the API's types don't carry its fields. */
export function geometryOf(values: unknown): Geometry | undefined {
  if (typeof values !== "object" || values === null) return undefined;
  const { kind, objective, binning, matrix, residual_px } = values as Record<string, unknown>;
  const fits =
    kind === "geometry" &&
    typeof objective === "string" &&
    typeof binning === "number" &&
    binning >= 1 &&
    typeof residual_px === "number" &&
    Array.isArray(matrix) &&
    matrix.length === 2 &&
    matrix.every(isPair);
  return fits ? { objective, binning, matrix: matrix as Matrix, residual_px } : undefined;
}

/**
 * The stage's move, in µm along x and y, that brings what the image shows
 * at the sensor's point `from` to its point `to`. The view counts in the
 * sensor's pixels, and the matrix in the image's, `binning` of them each.
 */
export function moveFor(
  geometry: Geometry,
  from: [number, number],
  to: [number, number],
): Partial<Xyz> {
  const [[a, b], [c, d]] = geometry.matrix;
  const dx = (from[0] - to[0]) / geometry.binning;
  const dy = (from[1] - to[1]) / geometry.binning;
  return { x: a * dx + b * dy, y: c * dx + d * dy };
}

/**
 * A sensor pixel's side at the sample, in µm, as the geometry measured it
 * (W8.2): √|det M| is the image's pixel, `binning` of the sensor's wide.
 */
export function sideOf(geometry: Geometry): number {
  const [[a, b], [c, d]] = geometry.matrix;
  return Math.sqrt(Math.abs(a * d - b * c)) / geometry.binning;
}

/**
 * Whether an alarm's reason is the controller waiting to be homed, as after
 * a start: the simulator's words, and grblHAL's. Homing that failed is an
 * alarm like any other, its reason said.
 */
export function waitsForHoming(reason: string): boolean {
  return /\bhoming required\b|\bneeds? homing\b|\bnot homed\b/i.test(reason);
}
