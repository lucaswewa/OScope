import { describe, expect, it } from "vitest";

import { curveOf, peakOf, type FocusReport } from "./curve";
import recorded from "./recorded-last.json";

/** A stepped run's report, over the simulator's tissue, 12 µm above its focus. */
const report = recorded as FocusReport;
const names = { coarse: "Coarse", fine: "Fine", focus: "Focus" };

describe("autofocus's curve, from a recorded run", () => {
  it("draws each sweep in order of z, from where the run started", () => {
    const [coarse, fine] = curveOf(report, names);
    expect(coarse!.kind).toBe("line");
    expect(coarse!.x).toHaveLength(report.coarse.length);
    const xs = Array.from(coarse!.x);
    expect(xs).toEqual([...xs].sort((a, b) => a - b));
    expect(Math.min(...xs)).toBeCloseTo(
      Math.min(...report.coarse.map((p) => p.z_um)) - report.start_um,
    );
    expect(fine!.kind).toBe("points");
    expect(fine!.y).toHaveLength(report.fine.length);
  });

  it("marks the focus found across the measures, near the sharpest point", () => {
    expect(report.found).toBe(true);
    const marked = curveOf(report, names).find((one) => one.name === "Focus")!;
    const at = report.z_um - report.start_um;
    expect(Array.from(marked.x)).toEqual([at, at]);
    const values = [...report.coarse, ...report.fine].map((p) => p.value);
    expect(Array.from(marked.y)).toEqual([Math.min(...values), Math.max(...values)]);
    // It was 12 µm above: the focus is about 12 µm down, within the depth of field of the peak.
    expect(at).toBeCloseTo(-12, -1);
    expect(Math.abs(peakOf(report)!.z_um - report.z_um)).toBeLessThan(report.depth_of_field_um);
  });

  it("marks nothing when no focus was found, and draws no fine sweep a run didn't make", () => {
    const refused = { ...report, found: false, refused: "an empty field" };
    expect(curveOf(refused, names).map((one) => one.name)).toEqual(["Coarse", "Fine"]);
    const continuous = { ...report, sweep: "continuous" as const, fine: [] };
    expect(curveOf(continuous, names).map((one) => one.name)).toEqual(["Coarse", "Focus"]);
    expect(peakOf({ ...report, coarse: [], fine: [] })).toBeUndefined();
  });
});
