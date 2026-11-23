// A scan's positions and stacks (W5.4): the list's editing, each place
// added with its z and a name of its own, renamed, moved and removed, and
// sent as the server takes it; and a stack's planes, as the server counts
// them.
import { describe, expect, it } from "vitest";

import { added, moved, planesOf, positionsOf, removed, renamed, type Place } from "./positions";

const nameOf = (n: number) => `Position ${n}`;
const three: Place[] = [
  { x: 1, y: 2, z: 3, name: "A" },
  { x: 4, y: 5, name: "B" },
  { x: 6, y: 7, z: 8, name: "C" },
];

describe("a scan's positions", () => {
  it("are added last, with the stage's z, each named as no other is", () => {
    const one = added([], { x: 10, y: 20, z: 30 }, nameOf);
    expect(one).toEqual([{ x: 10, y: 20, z: 30, name: "Position 1" }]);
    // Position 2 taken by a rename: the next free.
    const taken = [{ x: 0, y: 0, name: "Position 2" }];
    expect(added(taken, { x: 1, y: 1 }, nameOf).at(-1)).toEqual({ x: 1, y: 1, name: "Position 3" });
  });

  it("are renamed, moved within the list, and removed", () => {
    expect(renamed(three, 1, "Middle").map((p) => p.name)).toEqual(["A", "Middle", "C"]);
    const names = (list: Place[]) => list.map((p) => p.name).join("");
    expect(names(moved(three, 2, 0))).toBe("CAB");
    expect(names(moved(three, 0, 1))).toBe("BAC");
    expect(names(moved(three, 1, 2))).toBe("ACB");
    // Past the ends, at the ends; nowhere, unchanged.
    expect(names(moved(three, 0, -1))).toBe("ABC");
    expect(names(moved(three, 1, 9))).toBe("ACB");
    expect(names(moved(three, 5, 0))).toBe("ABC");
    expect(names(removed(three, 1))).toBe("AC");
  });

  it("go to the server with their z and their names, a blank one left out", () => {
    expect(positionsOf([...three.slice(0, 2), { x: 6, y: 7, name: "  " }])).toEqual([
      { x_um: 1, y_um: 2, z_um: 3, name: "A" },
      { x_um: 4, y_um: 5, name: "B" },
      { x_um: 6, y_um: 7 },
    ]);
  });
});

describe("a stack's planes", () => {
  it("are counted from its lowest, a step apart, to its highest that doesn't pass its top", () => {
    expect(planesOf({ below_um: 10, above_um: 10, step_um: 1 })).toBe(21);
    expect(planesOf({ below_um: 10, above_um: 10, step_um: 2 })).toBe(11);
    expect(planesOf({ below_um: 5, above_um: 4, step_um: 2 })).toBe(5);
    // Steps that don't divide exactly in floating point still reach the top.
    expect(planesOf({ below_um: 0.3, above_um: 0.3, step_um: 0.1 })).toBe(7);
    expect(planesOf({ below_um: 0, above_um: 0, step_um: 1 })).toBe(1);
  });

  it("are none for a stack that can't be", () => {
    expect(planesOf({ below_um: 10, above_um: 10, step_um: 0 })).toBe(0);
    expect(planesOf({ below_um: -1, above_um: 10, step_um: 1 })).toBe(0);
  });
});
