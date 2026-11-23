import { describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";

import DataTable from "@/ui/DataTable.vue";
import type { Column, Key, Sort } from "@/ui/table";
import { draw } from "@/ui/test-kit";

interface Scan {
  id: number;
  name: string;
  tiles: number;
}

const columns: Column<Scan>[] = [
  { key: "id", label: "Number", width: 80, align: "end", sortable: true },
  { key: "name", label: "Name", sortable: true },
  { key: "tiles", label: "Tiles", min: 60 },
];
const scans: Scan[] = Array.from({ length: 50 }, (_, at) => ({
  id: at + 1,
  name: `scan ${50 - at}`,
  tiles: (at * 7) % 13,
}));

/** A table of the scans, held as a page holds it, and what it said. */
function table(props: Record<string, unknown> = {}) {
  const said = {
    sort: [] as (Sort | null)[],
    selected: [] as Key[][],
    widths: [] as Record<string, number>[],
  };
  const state = {
    sort: null as Sort | null,
    selected: [] as Key[],
    widths: {} as Record<string, number>,
  };
  const drawn = draw(DataTable, {
    columns,
    rows: scans,
    rowKey: (row: Scan) => row.id,
    label: "Scans",
    picking: "multiple",
    ...props,
    sort: state.sort,
    selected: state.selected,
    widths: state.widths,
    "onUpdate:sort": (now: Sort | null) => {
      said.sort.push(now);
      state.sort = now;
      void drawn.set({ sort: now });
    },
    "onUpdate:selected": (now: Key[]) => {
      said.selected.push(now);
      state.selected = now;
      void drawn.set({ selected: now });
    },
    "onUpdate:widths": (now: Record<string, number>) => {
      said.widths.push(now);
      state.widths = now;
      void drawn.set({ widths: now });
    },
  });
  const grid = () => drawn.wrapper.get("[role='grid']");
  const cell = (row: number, col: number) =>
    drawn.wrapper.find(`[data-cell="${row}:${col}"]`).element as HTMLElement | undefined;
  return {
    ...drawn,
    said,
    state,
    grid,
    cell,
    async key(name: string, more: KeyboardEventInit = {}): Promise<void> {
      document.activeElement!.dispatchEvent(
        new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...more }),
      );
      await nextTick();
      await nextTick();
    },
    texts: (col: number) =>
      drawn.wrapper.findAll(`.body [role='row'] [data-cell$=":${col}"]`).map((one) => one.text()),
  };
}

describe("a data table", () => {
  it("is a grid that says how many rows and columns it has, and draws only some", () => {
    const scansTable = table();
    const grid = scansTable.grid();
    expect(grid.attributes()).toMatchObject({
      "aria-label": "Scans",
      "aria-rowcount": "51",
      "aria-colcount": "3",
      "aria-multiselectable": "true",
      tabindex: "-1",
    });
    const headers = grid.findAll("[role='columnheader']");
    expect(headers.map((one) => one.text())).toEqual(["Number", "Name", "Tiles"]);
    expect(headers.map((one) => one.attributes("aria-colindex"))).toEqual(["1", "2", "3"]);
    const rows = grid.findAll(".body [role='row']");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(50);
    expect(rows[0]!.attributes("aria-rowindex")).toBe("2");
    expect(rows[0]!.attributes("aria-selected")).toBe("false");
  });

  it("draws no more rows than the browser's window shows, where its page gave it no height", async () => {
    // Such a table grows to hold every row. The height is told where the
    // test's browser keeps it, up the elements' line.
    const many = Array.from({ length: 1000 }, (_, at) => ({
      id: at,
      name: `scan ${at}`,
      tiles: 0,
    }));
    let owner: object = HTMLDivElement.prototype;
    while (!Object.hasOwn(owner, "clientHeight")) owner = Object.getPrototypeOf(owner) as object;
    const tall = vi
      .spyOn(owner as HTMLElement, "clientHeight", "get")
      .mockReturnValue(31 + many.length * 30);
    try {
      const drawn = table({ rows: many });
      await nextTick();
      const rows = drawn.grid().findAll(".body [role='row']");
      expect(rows.length).toBeGreaterThan(0);
      expect(rows.length).toBeLessThan(Math.ceil(window.innerHeight / 30) + 10);
    } finally {
      tall.mockRestore();
    }
  });

  it("has one cell that Tab reaches: the first header's, at first", () => {
    const scansTable = table();
    const tabbable = scansTable.grid().findAll("[tabindex='0']");
    expect(tabbable.map((one) => one.attributes("data-cell"))).toEqual(["-1:0"]);
  });

  it("keeps a cell for Tab when the one with the focus scrolls away, and shows the first rows when sorted or sent to its start", async () => {
    const scansTable = table();
    const grid = scansTable.grid();
    const tabbable = () => grid.findAll("[tabindex='0']").map((one) => one.attributes("data-cell"));
    scansTable.cell(-1, 1)!.focus();
    await scansTable.key("ArrowDown");
    expect(tabbable()).toEqual(["0:1"]);
    grid.element.scrollTop = 600;
    await grid.trigger("scroll");
    // Row 0 isn't drawn now: the first row drawn, in the same column, stands in for it.
    expect(grid.find("[data-cell='0:1']").exists()).toBe(false);
    expect(tabbable()).toEqual([
      `${grid.get(".body [role='row'] [role='gridcell']").attributes("data-cell")!.split(":")[0]}:1`,
    ]);
    await grid.findAll("[role='columnheader']")[1]!.trigger("click");
    expect(grid.element.scrollTop).toBe(0);
    expect(scansTable.texts(1)[0]).toBe("scan 1");
    // On a header, with the rows scrolled away: Ctrl+Home shows the first again.
    grid.element.scrollTop = 600;
    await grid.trigger("scroll");
    scansTable.cell(-1, 2)!.focus();
    await scansTable.key("Home", { ctrlKey: true });
    expect(document.activeElement).toBe(scansTable.cell(-1, 0));
    expect(grid.element.scrollTop).toBe(0);
  });

  it("sorts by a header pressed, and the other way when pressed again", async () => {
    const scansTable = table();
    const name = scansTable.grid().findAll("[role='columnheader']")[1]!;
    await name.trigger("click");
    expect(scansTable.said.sort).toEqual([{ key: "name", descending: false }]);
    expect(name.attributes("aria-sort")).toBe("ascending");
    expect(scansTable.texts(1).slice(0, 3)).toEqual(["scan 1", "scan 2", "scan 3"]);
    await name.trigger("click");
    expect(name.attributes("aria-sort")).toBe("descending");
    expect(scansTable.texts(1)[0]).toBe("scan 50");
    // A column that doesn't sort, doesn't.
    await scansTable.grid().findAll("[role='columnheader']")[2]!.trigger("click");
    expect(scansTable.said.sort).toHaveLength(2);
  });

  it("picks rows with a press: on its own, with Ctrl one more, with Shift those between", async () => {
    const scansTable = table();
    const press = (row: number, more: MouseEventInit = {}) =>
      scansTable.cell(row, 0)!.dispatchEvent(new MouseEvent("click", { bubbles: true, ...more }));
    press(1);
    await nextTick();
    press(4, { shiftKey: true });
    await nextTick();
    expect(scansTable.state.selected).toEqual([2, 3, 4, 5]);
    press(2, { ctrlKey: true });
    await nextTick();
    expect(scansTable.state.selected).toEqual([2, 4, 5]);
    press(0);
    await nextTick();
    expect(scansTable.state.selected).toEqual([1]);
    const first = scansTable.grid().find("[aria-rowindex='2']");
    expect(first.attributes("aria-selected")).toBe("true");
  });

  it("moves its focus by the grid's keys, and picks with Space and Ctrl+A", async () => {
    const scansTable = table();
    scansTable.cell(-1, 0)!.focus();
    await scansTable.key("ArrowDown");
    expect(document.activeElement).toBe(scansTable.cell(0, 0));
    await scansTable.key("ArrowRight");
    expect(document.activeElement).toBe(scansTable.cell(0, 1));
    // Tab now comes back here, and only here.
    expect(
      scansTable
        .grid()
        .findAll("[tabindex='0']")
        .map((one) => one.attributes("data-cell")),
    ).toEqual(["0:1"]);
    await scansTable.key(" ");
    expect(scansTable.state.selected).toEqual([1]);
    await scansTable.key("ArrowDown");
    await scansTable.key(" ");
    expect(scansTable.state.selected).toEqual([1, 2]);
    await scansTable.key("a", { ctrlKey: true });
    expect(scansTable.state.selected).toHaveLength(50);
    // To the grid's last cell, and back to its first: the first rows shown again.
    await scansTable.key("End", { ctrlKey: true });
    expect(document.activeElement).toBe(scansTable.cell(49, 2));
    expect(scansTable.grid().element.scrollTop).toBeGreaterThan(0);
    await scansTable.key("Home", { ctrlKey: true });
    expect(document.activeElement).toBe(scansTable.cell(-1, 0));
    expect(scansTable.grid().element.scrollTop).toBe(0);
  });

  it("sorts by Enter on a header, and changes a column's width by Shift and the arrows", async () => {
    const scansTable = table();
    scansTable.cell(-1, 0)!.focus();
    await scansTable.key("Enter");
    expect(scansTable.state.sort).toEqual({ key: "id", descending: false });
    await scansTable.key("ArrowRight", { shiftKey: true });
    expect(scansTable.state.widths).toEqual({ id: 96 });
    await scansTable.key("ArrowRight");
    await scansTable.key("ArrowLeft", { shiftKey: true });
    await scansTable.key("ArrowLeft", { shiftKey: true });
    // 160 - 32; and no narrower than the column's least, nor the table's.
    expect(scansTable.state.widths).toEqual({ id: 96, name: 128 });
    for (let n = 0; n < 12; n++) await scansTable.key("ArrowLeft", { shiftKey: true });
    expect(scansTable.state.widths.name).toBe(48);
    await scansTable.key("ArrowRight");
    for (let n = 0; n < 12; n++) await scansTable.key("ArrowLeft", { shiftKey: true });
    expect(scansTable.state.widths.tiles).toBe(60);
    expect(scansTable.grid().attributes("style")).toContain("--columns: 96px 48px 60px");
  });

  it("picks nothing where nothing may be picked, and says nothing of picking", async () => {
    const plain = table({ picking: "none" });
    expect(plain.grid().attributes("aria-multiselectable")).toBeUndefined();
    expect(plain.grid().find(".body [role='row']").attributes("aria-selected")).toBeUndefined();
    plain.cell(0, 0)!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await nextTick();
    plain.cell(0, 0)!.focus();
    await plain.key(" ");
    await plain.key("a", { ctrlKey: true });
    expect(plain.said.selected).toEqual([]);
  });

  it("says so when it has nothing to show", () => {
    const empty = table({ rows: [] });
    expect(empty.grid().attributes("aria-rowcount")).toBe("1");
    expect(empty.grid().find(".nothing").text()).toBe("Nothing to show.");
    expect(empty.grid().findAll(".body [role='row']")).toHaveLength(0);
  });
});
