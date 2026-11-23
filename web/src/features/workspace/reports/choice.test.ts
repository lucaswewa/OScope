// The results a report is on (W7.9): every one at first; kept as the
// project's change, a new one ticked only when every one was; what is asked
// of the server, in the project's order, or why it can't be; and the report
// a job made.
import { describe, expect, it } from "vitest";

import { everyOf, keptOf, madeReport, makingOf } from "./choice";

const results = (...ids: string[]) => ids.map((id) => ({ id }));

describe("the choice of results", () => {
  it("ticks every result at first", () => {
    expect([...everyOf(results("a", "b", "c"))]).toEqual(["a", "b", "c"]);
    expect(everyOf([]).size).toBe(0);
  });

  it("unticks a result gone, and ticks a new one only when every one was ticked", () => {
    const before = results("a", "b", "c");
    const now = results("a", "c", "d");
    // Every one ticked: the new one too, the one gone not.
    expect([...keptOf(new Set(["a", "b", "c"]), before, now)]).toEqual(["a", "c", "d"]);
    // Some unticked: the new one is left alone, and the ticks stay.
    expect([...keptOf(new Set(["a", "b"]), before, now)]).toEqual(["a"]);
    expect([...keptOf(new Set(), before, now)]).toEqual([]);
    // From none at all, nothing was every one.
    expect([...keptOf(new Set(), [], now)]).toEqual([]);
  });

  it("asks for the results in the project's order, through the template, PDF before HTML", () => {
    const all = results("a", "b", "c");
    expect(makingOf(all, new Set(["c", "a"]), "mine", ["html", "pdf"])).toEqual({
      results: ["a", "c"],
      template: "mine",
      formats: ["pdf", "html"],
    });
    // A tick of a result no longer listed asks for nothing.
    expect(makingOf(all, new Set(["b", "gone"]), "standard", ["html"])).toEqual({
      results: ["b"],
      template: "standard",
      formats: ["html"],
    });
  });

  it("says why nothing can be asked: no result, no form, or no template", () => {
    const all = results("a");
    expect(makingOf(all, new Set(), "standard", ["pdf"])).toEqual({ why: "noResult" });
    expect(makingOf(all, new Set(["gone"]), "standard", ["pdf"])).toEqual({ why: "noResult" });
    expect(makingOf(all, new Set(["a"]), "standard", [])).toEqual({ why: "noFormat" });
    expect(makingOf(all, new Set(["a"]), "", ["pdf"])).toEqual({ why: "noTemplate" });
  });

  it("finds the report a job made among its results' paths", () => {
    expect(madeReport(["projects/p/results/r", "projects/p/reports/n1"])).toBe("n1");
    expect(madeReport(["projects/p/results/r"])).toBeUndefined();
    expect(madeReport([])).toBeUndefined();
  });
});
