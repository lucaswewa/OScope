// A number field's rules (ADR-0042), apart from the field that shows them:
// reading what was typed, stepping, and writing a value.

/** The values a field takes: from `min` to `max`, either of which may be absent. */
export interface Range {
  min?: number | undefined;
  max?: number | undefined;
}

/** Why what was typed isn't taken: each has a sentence in the components' words. */
export type Refusal = "needed" | "notANumber" | "least" | "most";

export type Read = { ok: true; value: number | null } | { ok: false; why: Refusal };

/** A number as people write one: digits, a point, a sign, an exponent. Nothing else. */
const WRITTEN = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i;

/**
 * Reads what was typed: a number in the range, or why not. Nothing typed is
 * no value, which only an optional field takes. A value outside the range is
 * refused, not moved into it: the user is told, and decides.
 */
export function read(text: string, range: Range, optional = false): Read {
  const typed = text.trim();
  if (typed === "") return optional ? { ok: true, value: null } : { ok: false, why: "needed" };
  const value = WRITTEN.test(typed) ? Number(typed) : Number.NaN;
  if (!Number.isFinite(value)) return { ok: false, why: "notANumber" };
  if (range.min !== undefined && value < range.min) return { ok: false, why: "least" };
  if (range.max !== undefined && value > range.max) return { ok: false, why: "most" };
  // A typed "-0" is 0: the field writes one zero, not two.
  return { ok: true, value: value || 0 };
}

/** How many decimals a number is written with: 2 for 0.25, 7 for 1e-7. */
export function decimals(value: number): number {
  const [mantissa = "", exponent = "0"] = String(value).toLowerCase().split("e");
  const fraction = mantissa.split(".")[1]?.length ?? 0;
  return Math.max(0, fraction - Number(exponent));
}

/**
 * `value` moved by `steps` of `step`, and kept in the range. The sum is
 * rounded to the decimals its parts have, so 0.1 and 0.2 make 0.3; and a
 * sum that rounds to nothing is 0, not the -0 that would be written "-0.00".
 */
export function stepped(value: number, steps: number, step: number, range: Range): number {
  const digits = Math.min(Math.max(decimals(value), decimals(step)), 100);
  const moved = Number((value + steps * step).toFixed(digits)) || 0;
  return Math.min(Math.max(moved, range.min ?? -Infinity), range.max ?? Infinity);
}

/** A value as the field writes it: with `digits` decimals where given, else as it is. */
export function written(value: number | null, digits?: number): string {
  if (value === null) return "";
  return digits === undefined ? String(value) : value.toFixed(digits);
}
