// The data table's rules (ADR-0042), apart from the table that shows them:
// the order of its rows, the window of them a scroll shows, where the grid's
// keys take the focus, and which rows a press picks.

/** What a row goes by. */
export type Key = string | number;

export interface Column<T> {
  key: string;
  label: string;
  /** Its width at first, in pixels. */
  width?: number;
  /** The narrowest it can be made, in pixels. */
  min?: number;
  /** Numbers line up at the end of their cells. */
  align?: "start" | "end";
  sortable?: boolean;
  /** What the cell shows: the row's value under `key`, written as it is, when not given. */
  text?: (row: T) => string;
  /** What the column sorts by: the row's value under `key`, when not given. */
  value?: (row: T) => string | number;
}

export interface Sort {
  key: string;
  descending: boolean;
}

/** The width a column has unless it is given one. */
export const WIDTH = 160;

/** The narrowest a column can be made unless it says. */
export const MIN = 48;

const collator = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

/** A row's value under a column's key. */
export function valueOf<T>(row: T, column: Column<T>): string | number {
  if (column.value) return column.value(row);
  const value = (row as Record<string, unknown>)[column.key];
  return typeof value === "number" ? value : String(value ?? "");
}

/** What a cell shows. */
export function textOf<T>(row: T, column: Column<T>): string {
  return column.text ? column.text(row) : String(valueOf(row, column));
}

/**
 * The rows in the order `sort` asks: numbers by size, words as people order
 * them ("scan 2" before "scan 10"). Rows that tie keep their order. Without a
 * sort, or with one by a column it doesn't know, they are as given.
 */
export function sorted<T>(
  rows: readonly T[],
  sort: Sort | null,
  columns: readonly Column<T>[],
): readonly T[] {
  const column = sort && columns.find((one) => one.key === sort.key);
  if (!sort || !column) return rows;
  const keyed = rows.map((row, at) => ({ row, at, by: valueOf(row, column) }));
  const sign = sort.descending ? -1 : 1;
  keyed.sort((a, b) => {
    const order =
      typeof a.by === "number" && typeof b.by === "number"
        ? a.by - b.by
        : collator.compare(String(a.by), String(b.by));
    return order * sign || a.at - b.at;
  });
  return keyed.map((one) => one.row);
}

/** What activating a column's header does to the sort: another column ascends; the same one turns round. */
export function nextSort(sort: Sort | null, key: string): Sort {
  return sort?.key === key ? { key, descending: !sort.descending } : { key, descending: false };
}

/**
 * The rows a scroll shows, from `first` up to but not including `last`, with
 * `extra` more on each side, so that a quick scroll finds them drawn.
 */
export function windowOf(
  scrollTop: number,
  height: number,
  rowHeight: number,
  count: number,
  extra = 5,
): { first: number; last: number } {
  if (count <= 0 || rowHeight <= 0) return { first: 0, last: 0 };
  const top = Math.min(Math.max(0, Math.floor(scrollTop / rowHeight)), count - 1);
  const seen = Math.ceil(Math.max(0, height) / rowHeight) + 1;
  return { first: Math.max(0, top - extra), last: Math.min(count, top + seen + extra) };
}

/** A cell of the grid: `row` -1 is the header's. */
export interface Cell {
  row: number;
  col: number;
}

/**
 * Where a key takes the grid's focus (WAI-ARIA's grid pattern), among `rows`
 * rows under the header, `cols` columns, and `page` rows to a page. Nothing,
 * for a key it doesn't move by.
 */
export function moved(
  cell: Cell,
  key: string,
  ctrl: boolean,
  rows: number,
  cols: number,
  page: number,
): Cell | undefined {
  const row = (to: number): number => Math.min(Math.max(to, -1), rows - 1);
  switch (key) {
    case "ArrowRight":
      return { ...cell, col: Math.min(cell.col + 1, cols - 1) };
    case "ArrowLeft":
      return { ...cell, col: Math.max(cell.col - 1, 0) };
    case "ArrowDown":
      return { ...cell, row: row(cell.row + 1) };
    case "ArrowUp":
      return { ...cell, row: row(cell.row - 1) };
    case "PageDown":
      return { ...cell, row: row(cell.row + page) };
    case "PageUp":
      return { ...cell, row: row(cell.row - page) };
    case "Home":
      return ctrl ? { row: -1, col: 0 } : { ...cell, col: 0 };
    case "End":
      return ctrl ? { row: rows - 1, col: cols - 1 } : { ...cell, col: cols - 1 };
    default:
      return undefined;
  }
}

/** How many rows a table lets be picked. */
export type Picking = "none" | "single" | "multiple";

/** How a row is picked: on its own, added or taken away (Ctrl), or with those since the last (Shift). */
export type How = "only" | "toggle" | "range";

/**
 * The rows picked after `key`'s is, given those picked before, the row last
 * picked on its own, and every row's key in the order shown.
 */
export function picked(
  before: readonly Key[],
  key: Key,
  how: How,
  picking: Picking,
  anchor: Key | undefined,
  order: readonly Key[],
): Key[] {
  if (picking === "none") return [...before];
  if (picking === "single" || how === "only") return [key];
  if (how === "toggle") {
    return before.includes(key) ? before.filter((one) => one !== key) : [...before, key];
  }
  const from = anchor === undefined ? -1 : order.indexOf(anchor);
  const to = order.indexOf(key);
  if (from < 0 || to < 0) return [key];
  return order.slice(Math.min(from, to), Math.max(from, to) + 1);
}
