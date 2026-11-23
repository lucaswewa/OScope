// The results a report is on (W7.9): ticked from a project's, every one at
// first; kept as the project's results change, a new one ticked only when
// every one was; and what is asked of the server, or why it can't be.
import type { Format, Making } from "@/api/reports";

/** A result, as the choice needs it. */
export interface Choosable {
  id: string;
}

/** The ticks at first: every result. */
export function everyOf(results: readonly Choosable[]): Set<string> {
  return new Set(results.map((one) => one.id));
}

/**
 * The ticks once a project's results are `now`: those gone untick; a new
 * one is ticked if every one before it was, and left alone otherwise.
 */
export function keptOf(
  ticked: ReadonlySet<string>,
  before: readonly Choosable[],
  now: readonly Choosable[],
): Set<string> {
  const every = before.length > 0 && before.every((one) => ticked.has(one.id));
  const known = new Set(before.map((one) => one.id));
  return new Set(
    now.filter((one) => ticked.has(one.id) || (every && !known.has(one.id))).map((one) => one.id),
  );
}

/** What a report asks: its results in the project's order, its template and forms; or why it can't. */
export function makingOf(
  results: readonly Choosable[],
  ticked: ReadonlySet<string>,
  template: string,
  formats: readonly Format[],
): Making | { why: "noResult" | "noFormat" | "noTemplate" } {
  const chosen = results.filter((one) => ticked.has(one.id)).map((one) => one.id);
  if (chosen.length === 0) return { why: "noResult" };
  if (formats.length === 0) return { why: "noFormat" };
  if (!template) return { why: "noTemplate" };
  const order: Format[] = ["pdf", "html"];
  return { results: chosen, template, formats: order.filter((one) => formats.includes(one)) };
}

/** A report's ID, from the path its job made: `projects/{project}/reports/{id}`. */
export function madeReport(paths: readonly string[]): string | undefined {
  const path = paths.find((one) => /\/reports\/[^/]+$/.test(one));
  return path?.split("/").pop();
}
