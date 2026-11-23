// Two results compared (W7.4): what differs in how they were made, from the
// server's difference of their provenance, each place said by its node and
// parameter; and what differs in what they made, each table's rows, from
// their summaries; each number's change, by how much.
import type { Change, Summary } from "@/api/workspace";

/** A row of the comparison's table. */
export interface Row {
  /** Where: a node's parameter, `node: name`; another place; or a table's name. */
  place: string;
  /** Whether it is a table's rows, counted. */
  counted: boolean;
  before: string;
  after: string;
  /** By how much a number changed, signed; nothing for other values. */
  by?: string;
}

/**
 * A JSON Pointer's place, said: a node's parameter as `node: name`, its
 * node by its ID; others by their parts, the analysis's own place in the
 * provenance, `recipe/workspace.run`, left out.
 */
export function placeOf(path: string): string {
  const parts = path
    .split("/")
    .slice(1)
    .map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"));
  const at = parts.findIndex((part, i) => part === "nodes" && parts[i + 2] === "params");
  if (at >= 0 && parts.length > at + 3) return `${parts[at + 1]}: ${parts.slice(at + 3).join(".")}`;
  const own = parts[0] === "recipe" && parts[1] === "workspace.run" ? parts.slice(2) : parts;
  return own.join(" › ");
}

/** A value as the table shows it: none as a dash, text as it is, the rest as JSON. */
export function shown(value: unknown): string {
  if (value === null || value === undefined) return "—";
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** By how much `after` is `before`, both numbers: `+3`, `−0.5`, or nothing. */
export function by(before: unknown, after: unknown): string | undefined {
  if (typeof before !== "number" || typeof after !== "number") return undefined;
  const change = Number((after - before).toPrecision(12));
  return change > 0 ? `+${change}` : change < 0 ? `−${-change}` : "0";
}

const row = (place: string, counted: boolean, before: unknown, after: unknown): Row => {
  const change = by(before, after);
  return {
    place,
    counted,
    before: shown(before),
    after: shown(after),
    ...(change ? { by: change } : {}),
  };
};

/** Results `a` and `b` compared: `changes`, the server's, then their tables' rows that differ. */
export function comparison(changes: readonly Change[], a?: Summary, b?: Summary): Row[] {
  const rows = changes.map((change) =>
    row(placeOf(change.path), false, change.before, change.after),
  );
  const tables = [...new Set([...Object.keys(a?.rows ?? {}), ...Object.keys(b?.rows ?? {})])];
  for (const table of tables.sort()) {
    const [before, after] = [a?.rows[table], b?.rows[table]];
    if (before !== after) rows.push(row(table, true, before, after));
  }
  return rows;
}
