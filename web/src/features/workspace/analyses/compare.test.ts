// Two results compared (W7.4), from answers recorded from a server
// (scripts/record-analyses.mjs): discs counted from 10 µm², then from
// 300 µm², and the first made again.
import { describe, expect, it } from "vitest";

import type { Change, Summary } from "@/api/workspace";

import { by, comparison, placeOf, shown } from "./compare";
import recorded from "./recorded-analyses.json";

const changed = recorded.changed as Change[];
const again = recorded.again as Change[];
const { a, b, again: made } = recorded.summaries as unknown as Record<string, Summary>;

describe("the comparison's table", () => {
  it("names a parameter changed by its node, and the counts, each by how much", () => {
    expect(comparison(changed, a, b)).toEqual([
      { place: "measure: min_area", counted: false, before: "10", after: "300", by: "+290" },
      { place: "objects", counted: true, before: "48", after: "24", by: "−24" },
    ]);
  });

  it("is empty for a result made again", () => {
    expect(again).toEqual([]);
    expect(comparison(again, a, made)).toEqual([]);
  });

  it("counts a table one result has and the other hasn't", () => {
    const none = { ...b!, rows: {} };
    expect(comparison([], a, none)).toEqual([
      { place: "objects", counted: true, before: "48", after: "—" },
    ]);
    expect(comparison([], undefined, undefined)).toEqual([]);
  });
});

describe("a place, a value and a change", () => {
  it("are said plainly", () => {
    expect(placeOf("/recipe/workspace.run/roi/x")).toBe("roi › x");
    expect(placeOf("/recipe/workspace.run/recipe/nodes/a~1b/params/pixel_um/0")).toBe(
      "a/b: pixel_um.0",
    );
    expect(placeOf("/hashes/abc")).toBe("hashes › abc");
    expect([shown(null), shown("he"), shown([1, 2]), shown(0.5)]).toEqual([
      "—",
      "he",
      "[1,2]",
      "0.5",
    ]);
    expect([by(0.1, 0.3), by(5, 5), by("a", "b"), by(null, 3)]).toEqual([
      "+0.2",
      "0",
      undefined,
      undefined,
    ]);
  });
});
