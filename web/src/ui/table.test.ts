import { describe, expect, it } from "vitest";

import {
  moved,
  nextSort,
  picked,
  sorted,
  textOf,
  valueOf,
  windowOf,
  type Column,
} from "@/ui/table";

interface Scan {
  name: string;
  tiles: number;
  when?: string;
}

const columns: Column<Scan>[] = [
  { key: "name", label: "Name", sortable: true },
  { key: "tiles", label: "Tiles", sortable: true, align: "end" },
  { key: "when", label: "When", text: (row) => `on ${row.when ?? "no day"}` },
];
const scans: Scan[] = [
  { name: "scan 10", tiles: 40 },
  { name: "Scan 2", tiles: 400, when: "Monday" },
  { name: "scan 1", tiles: 40 },
  { name: "atlas", tiles: 9 },
];
const names = (rows: readonly Scan[]): string[] => rows.map((row) => row.name);

describe("a column's values", () => {
  it("are the row's under its key, or what the column says", () => {
    expect(valueOf(scans[0]!, columns[0]!)).toBe("scan 10");
    expect(valueOf(scans[0]!, columns[1]!)).toBe(40);
    expect(valueOf(scans[0]!, columns[2]!)).toBe("");
    expect(textOf(scans[0]!, columns[1]!)).toBe("40");
    expect(textOf(scans[1]!, columns[2]!)).toBe("on Monday");
    expect(valueOf({ name: "x", tiles: 1 }, { key: "x", label: "X", value: () => 7 })).toBe(7);
  });
});

describe("sorting", () => {
  it("orders words as people do, numbers by size, either way", () => {
    expect(names(sorted(scans, { key: "name", descending: false }, columns))).toEqual([
      "atlas",
      "scan 1",
      "Scan 2",
      "scan 10",
    ]);
    expect(names(sorted(scans, { key: "name", descending: true }, columns))).toEqual([
      "scan 10",
      "Scan 2",
      "scan 1",
      "atlas",
    ]);
    expect(names(sorted(scans, { key: "tiles", descending: false }, columns))).toEqual([
      "atlas",
      "scan 10",
      "scan 1",
      "Scan 2",
    ]);
  });

  it("keeps the order of rows that tie, whichever way", () => {
    for (const descending of [false, true]) {
      const order = names(sorted(scans, { key: "tiles", descending }, columns));
      expect(order.indexOf("scan 10")).toBeLessThan(order.indexOf("scan 1"));
    }
  });

  it("leaves the rows as given without a sort, or with one it doesn't know, and changes nothing", () => {
    expect(sorted(scans, null, columns)).toBe(scans);
    expect(sorted(scans, { key: "colour", descending: false }, columns)).toBe(scans);
    const copy = [...scans];
    sorted(scans, { key: "name", descending: false }, columns);
    expect(scans).toEqual(copy);
  });

  it("goes by a column's header: another column ascends, the same one turns round", () => {
    expect(nextSort(null, "name")).toEqual({ key: "name", descending: false });
    expect(nextSort({ key: "name", descending: false }, "name")).toEqual({
      key: "name",
      descending: true,
    });
    expect(nextSort({ key: "name", descending: true }, "name")).toEqual({
      key: "name",
      descending: false,
    });
    expect(nextSort({ key: "name", descending: true }, "tiles")).toEqual({
      key: "tiles",
      descending: false,
    });
  });

  it("sorts 100,000 rows in well under a second", () => {
    const many = Array.from({ length: 100_000 }, (_, at) => ({
      name: `scan ${(at * 7919) % 100_000}`,
      tiles: (at * 31) % 997,
    }));
    const started = performance.now();
    const order = sorted(many, { key: "name", descending: false }, columns);
    expect(performance.now() - started).toBeLessThan(1000);
    expect(order[0]!.name).toBe("scan 0");
    expect(order[1]!.name).toBe("scan 1");
    expect(order.at(-1)!.name).toBe("scan 99999");
  });
});

describe("the virtual list's window", () => {
  it("holds the rows in view, with some more each side", () => {
    // 30 px rows, 300 px of view, scrolled to the top.
    expect(windowOf(0, 300, 30, 100_000)).toEqual({ first: 0, last: 16 });
    // Scrolled to row 1,000.
    expect(windowOf(30_000, 300, 30, 100_000)).toEqual({ first: 995, last: 1016 });
    // Part way into a row: the row it shows part of counts.
    expect(windowOf(30_015, 300, 30, 100_000)).toEqual({ first: 995, last: 1016 });
    expect(windowOf(30_000, 300, 30, 100_000, 0)).toEqual({ first: 1000, last: 1011 });
  });

  it("stops at the list's ends", () => {
    expect(windowOf(2_999_700, 300, 30, 100_000)).toEqual({ first: 99_985, last: 100_000 });
    // Scrolled past the end, as a list that shrank may be.
    expect(windowOf(9_000_000, 300, 30, 100_000)).toEqual({ first: 99_994, last: 100_000 });
    expect(windowOf(0, 300, 30, 3)).toEqual({ first: 0, last: 3 });
  });

  it("holds nothing for no rows, and stays sane for a list with no height", () => {
    expect(windowOf(0, 300, 30, 0)).toEqual({ first: 0, last: 0 });
    expect(windowOf(0, 300, 0, 10)).toEqual({ first: 0, last: 0 });
    // No height: the row at the top counts as seen, with the five more.
    expect(windowOf(-50, 0, 30, 10)).toEqual({ first: 0, last: 6 });
  });
});

describe("the grid's keys", () => {
  const at = { row: 5, col: 1 };
  const go = (key: string, ctrl = false, from = at) => moved(from, key, ctrl, 100, 4, 10);

  it("move by a cell, and stop at the grid's edges", () => {
    expect(go("ArrowRight")).toEqual({ row: 5, col: 2 });
    expect(go("ArrowLeft")).toEqual({ row: 5, col: 0 });
    expect(go("ArrowDown")).toEqual({ row: 6, col: 1 });
    expect(go("ArrowUp")).toEqual({ row: 4, col: 1 });
    expect(go("ArrowLeft", false, { row: 5, col: 0 })).toEqual({ row: 5, col: 0 });
    expect(go("ArrowRight", false, { row: 5, col: 3 })).toEqual({ row: 5, col: 3 });
    expect(go("ArrowDown", false, { row: 99, col: 1 })).toEqual({ row: 99, col: 1 });
  });

  it("reach the header above the first row, and no further", () => {
    expect(go("ArrowUp", false, { row: 0, col: 2 })).toEqual({ row: -1, col: 2 });
    expect(go("ArrowUp", false, { row: -1, col: 2 })).toEqual({ row: -1, col: 2 });
    expect(go("ArrowDown", false, { row: -1, col: 2 })).toEqual({ row: 0, col: 2 });
  });

  it("move a page at a time, and to the row's ends and the grid's", () => {
    expect(go("PageDown")).toEqual({ row: 15, col: 1 });
    expect(go("PageUp")).toEqual({ row: -1, col: 1 });
    expect(go("PageDown", false, { row: 95, col: 1 })).toEqual({ row: 99, col: 1 });
    expect(go("Home")).toEqual({ row: 5, col: 0 });
    expect(go("End")).toEqual({ row: 5, col: 3 });
    expect(go("Home", true)).toEqual({ row: -1, col: 0 });
    expect(go("End", true)).toEqual({ row: 99, col: 3 });
  });

  it("ignore what they don't move by", () => {
    expect(go("Enter")).toBeUndefined();
    expect(go("a")).toBeUndefined();
  });

  it("keep to the header in a grid with no rows", () => {
    expect(moved({ row: -1, col: 0 }, "ArrowDown", false, 0, 4, 10)).toEqual({ row: -1, col: 0 });
    expect(moved({ row: -1, col: 0 }, "End", true, 0, 4, 10)).toEqual({ row: -1, col: 3 });
  });
});

describe("picking rows", () => {
  const order = ["a", "b", "c", "d", "e"];

  it("picks one on its own, whatever was picked", () => {
    expect(picked(["a", "b"], "c", "only", "multiple", "a", order)).toEqual(["c"]);
    expect(picked(["a"], "c", "toggle", "single", "a", order)).toEqual(["c"]);
    expect(picked(["a"], "c", "range", "single", "a", order)).toEqual(["c"]);
  });

  it("adds and takes away one with Ctrl, where several may be picked", () => {
    expect(picked(["a"], "c", "toggle", "multiple", "a", order)).toEqual(["a", "c"]);
    expect(picked(["a", "c"], "a", "toggle", "multiple", "a", order)).toEqual(["c"]);
  });

  it("picks those since the last picked on its own with Shift, either way", () => {
    expect(picked(["b"], "d", "range", "multiple", "b", order)).toEqual(["b", "c", "d"]);
    expect(picked(["d"], "b", "range", "multiple", "d", order)).toEqual(["b", "c", "d"]);
    // With nothing to start from, the row alone.
    expect(picked([], "d", "range", "multiple", undefined, order)).toEqual(["d"]);
    expect(picked([], "d", "range", "multiple", "gone", order)).toEqual(["d"]);
  });

  it("picks nothing where nothing may be picked", () => {
    expect(picked(["a"], "c", "only", "none", "a", order)).toEqual(["a"]);
  });
});
