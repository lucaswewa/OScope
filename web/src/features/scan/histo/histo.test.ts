// The histo scan (W5.3): the steps' state; the tissue's islands from an
// overview's runs, and the regions kept as one mask; and, from a histo
// session recorded on the simulator (scripts/record-histo.mjs), the focus
// map's points as the scan engine chooses them, and the surface the run
// fitted through those it measured, a plane's and a thin plate's.
import { describe, expect, it } from "vitest";

import type { OverviewView } from "../overview";
import type { Plan } from "../plan";
import { islandsOf, mapPointsOf, maskOf, proposedOf, reachable, zAt, type FocusMap } from "./histo";
import recorded from "./recorded-histo.json";

/** An overview of 10 × 8 pixels of 10 µm from (1000, 2000), its tissue `runs`. */
function overviewOf(runs: [number, number, number][]): OverviewView {
  return {
    origin: { x: 1000, y: 2000 },
    pixel_um: 10,
    width: 10,
    height: 8,
    frames: 4,
    image: "",
    threshold: 0.2,
    kept: 2,
    dropped: 0,
    area_mm2: 0.01,
    tissue: runs,
  };
}

// Two islands: one of three rows, joined diagonally on its last; one alone.
const view = overviewOf([
  [1, 1, 4],
  [2, 2, 3],
  [3, 3, 5],
  [1, 7, 9],
]);

describe("the histo scan's steps", () => {
  it("each need what the one before made", () => {
    const flow = { regions: [] };
    expect(reachable("overview", flow)).toBe(true);
    expect(reachable("tissue", flow)).toBe(false);
    expect(reachable("tissue", { ...flow, overview: view })).toBe(true);
    expect(reachable("regions", { ...flow, overview: overviewOf([]) })).toBe(false);
    expect(reachable("regions", { ...flow, overview: view })).toBe(true);
    const regions = proposedOf(view);
    expect(reachable("focus", { overview: view, regions })).toBe(true);
    const dropped = regions.map((one) => ({ ...one, kept: false }));
    expect(reachable("run", { overview: view, regions: dropped })).toBe(false);
  });
});

describe("the tissue's islands", () => {
  it("are its runs joined where they touch, diagonally too, the largest first", () => {
    expect(islandsOf(view)).toEqual([
      { x: 1010, y: 2010, width: 40, height: 30 },
      { x: 1070, y: 2010, width: 20, height: 10 },
    ]);
  });

  it("are kept apart where they only come near", () => {
    const apart = overviewOf([
      [0, 0, 2],
      [2, 0, 2],
    ]);
    expect(islandsOf(apart)).toHaveLength(2);
  });

  it("are proposed as regions, each kept", () => {
    expect(proposedOf(view).every((one) => one.kept)).toBe(true);
  });
});

describe("the regions kept", () => {
  it("are one mask in the overview's pixels, over their bounds", () => {
    const [big, small] = proposedOf(view);
    const mask = maskOf(view, [big!, { ...small!, kept: false }])!;
    expect(mask).toMatchObject({ origin: { x: 1010, y: 2010 }, cell_um: 10, width: 4, height: 3 });
    expect(mask.cells.every(Boolean)).toBe(true);
    const both = maskOf(view, [big!, small!])!;
    expect([both.width, both.height]).toEqual([8, 3]);
    // Between the two, glass: out.
    expect(both.cells.slice(0, 8)).toEqual([true, true, true, true, false, false, true, true]);
    expect(maskOf(view, [{ ...big!, kept: false }])).toBeUndefined();
  });
});

describe("the focus map", () => {
  const plan = recorded.preview.plan as unknown as Plan;
  const { plane, thinPlate } = recorded as unknown as { plane: FocusMap; thinPlate: FocusMap };

  it("is measured where the scan engine would, the tile nearest each middle of its grid", () => {
    const points = mapPointsOf(plan, 2, 2);
    expect(points.length).toBeGreaterThanOrEqual(3);
    const tiles = plan.tiles.map((t) => `${t.x_um},${t.y_um}`);
    for (const p of points) expect(tiles).toContain(`${p.x},${p.y}`);
    // As the run measured them.
    const measured = [...plane.used, ...plane.rejected].map((p) => `${p.x_um},${p.y_um}`);
    expect(points.map((p) => `${p.x},${p.y}`).sort()).toEqual(
      measured
        .map((m) => {
          const [x, y] = m.split(",").map(Number);
          const nearest = plan.tiles.reduce((best, t) =>
            Math.hypot(t.x_um - x!, t.y_um - y!) < Math.hypot(best.x_um - x!, best.y_um - y!)
              ? t
              : best,
          );
          return `${nearest.x_um},${nearest.y_um}`;
        })
        .sort(),
    );
  });

  it("is a plane through its points, near each within how much it misses them", () => {
    expect(plane.surface.kind).toBe("plane");
    expect(zAt(plane, plane.centre)).toBeCloseTo(plane.coefficients[0]!, 9);
    // It rises by its slopes, in µm a mm: along x, then along y.
    const { x, y } = plane.centre;
    const at = zAt(plane, plane.centre);
    expect(zAt(plane, { x: x + 1000, y }) - at).toBeCloseTo(plane.coefficients[1]!, 9);
    expect(zAt(plane, { x, y: y + 1000 }) - at).toBeCloseTo(plane.coefficients[2]!, 9);
    const misses = plane.used.map((p) => zAt(plane, { x: p.x_um, y: p.y_um }) - p.z_um);
    const rms = Math.sqrt(misses.reduce((s, m) => s + m * m, 0) / misses.length);
    expect(rms).toBeLessThan(1);
  });

  it("is a thin plate through each of its points, as its smoothing of 0 has it", () => {
    expect(thinPlate.surface.kind).toBe("thin_plate");
    for (const p of thinPlate.used) {
      expect(zAt(thinPlate, { x: p.x_um, y: p.y_um })).toBeCloseTo(p.z_um, 6);
    }
  });
});
