// A scan's plan, for its page (W5.1): drawn from the server's plan of a
// snake over the simulator's tissue, recorded (scripts/record-plan.mjs);
// its warnings; the frames an overview takes; and each pattern's request.
import { describe, expect, it } from "vitest";

import {
  COVERSLIP,
  frameOf,
  overviewFrames,
  rectFromMm,
  regionOf,
  requestOf,
  shapesOf,
  warningsOf,
  type Asked,
  type Preview,
} from "./plan";
import recorded from "./recorded-plan.json";

const preview = recorded.preview as Preview;
const { plan } = preview;
const travel = { x: recorded.capabilities.x, y: recorded.capabilities.y };
const region = {
  x: recorded.region.rect.origin.x,
  y: recorded.region.rect.origin.y,
  width: recorded.region.rect.size.width,
  height: recorded.region.rect.size.height,
};

describe("the plan drawn", () => {
  it("is each tile's frame in order, the first marked, then the path from tile to tile, then the region", () => {
    // The tiles given out of their order: the drawing puts them in it.
    const shapes = shapesOf({ ...plan, tiles: [...plan.tiles].reverse() }, region);
    const tiles = plan.tiles.length;
    expect(tiles).toBe(plan.rows * plan.columns);
    expect(shapes.map((s) => s.kind)).toEqual([
      "first",
      ...Array<string>(tiles - 1).fill("frame"),
      "path",
      "region",
    ]);
    const path = shapes[tiles]!.points;
    const inOrder = [...plan.tiles].sort((a, b) => a.index - b.index);
    expect(path).toEqual(inOrder.map((t) => ({ x: t.x_um, y: t.y_um })));
  });

  it("goes back and forth along the rows, as a snake does", () => {
    const inOrder = [...plan.tiles].sort((a, b) => a.index - b.index);
    const rows = new Map<number, typeof inOrder>();
    for (const tile of inOrder) rows.set(tile.row, [...(rows.get(tile.row) ?? []), tile]);
    expect(rows.size).toBe(plan.rows);
    const ways = [...rows.values()].map((row) => Math.sign(row.at(-1)!.x_um - row[0]!.x_um));
    for (let i = 1; i < ways.length; i++) expect(ways[i]).toBe(-ways[i - 1]!);
  });

  it("frames each tile by its field's corners about its centre, which overlap their neighbours'", () => {
    const tile = plan.tiles[0]!;
    const frame = frameOf(plan, tile);
    expect(frame).toEqual(
      plan.field.corners_um.map((c) => ({ x: tile.x_um + c.x, y: tile.y_um + c.y })),
    );
    // Along a row, the next tile is a step away, closer than a frame's width.
    const width = Math.max(...frame.map((p) => p.x)) - Math.min(...frame.map((p) => p.x));
    expect(plan.field.step_um.width).toBeLessThan(width);
    expect(plan.field.step_um.width).toBeCloseTo(width * (1 - plan.field.overlap), 0);
  });
});

describe("its warnings", () => {
  const room = { travel, calibrated: true, available: 1e15 };

  it("are none within the stage's travel, calibrated, with room", () => {
    expect(warningsOf(preview, room)).toEqual([]);
  });

  it("count the tiles whose frame reaches past the stage's travel", () => {
    // The travel ending in the region's middle, along x.
    const cut = { ...travel, x: { ...travel.x, max: region.x + region.width / 2 } };
    const [warning] = warningsOf(preview, { ...room, travel: cut });
    expect(warning?.kind).toBe("limits");
    const past = plan.tiles.filter((t) => frameOf(plan, t).some((p) => p.x > cut.x.max)).length;
    expect(warning).toEqual({ kind: "limits", tiles: past });
    expect(past).toBeGreaterThan(0);
    expect(past).toBeLessThan(plan.tiles.length);
  });

  it("say a field spaced by the nominal pixel, but only once the calibration is known", () => {
    expect(warningsOf(preview, { ...room, calibrated: false })).toEqual([{ kind: "calibration" }]);
    expect(warningsOf(preview, { ...room, calibrated: undefined })).toEqual([]);
  });

  it("say a recording larger than the scratch disk's room", () => {
    const available = preview.estimate.bytes - 1;
    expect(warningsOf(preview, { ...room, available })).toEqual([
      { kind: "room", needed: preview.estimate.bytes, available },
    ]);
  });
});

describe("an overview", () => {
  it("takes frames a field apart over its region, every part of it covered", () => {
    // The 10×'s field on the MIChrome 20's sensor: 5472 × 3648 pixels of 0.24 µm.
    expect(overviewFrames(COVERSLIP, { width: 1313.28, height: 875.52 })).toBe(39 * 28);
    expect(overviewFrames(COVERSLIP, { width: 50_000, height: 24_000 })).toBe(1);
    expect(overviewFrames(COVERSLIP, { width: 0, height: 10 })).toBe(0);
  });

  it("starts over a standard slide's coverslip, typed in mm", () => {
    expect(rectFromMm(22, 0.5, 50, 24)).toEqual(COVERSLIP);
    expect(rectFromMm(22, 0.5, 50, null)).toBeUndefined();
    expect(rectFromMm(22, 0.5, 0, 24)).toBeUndefined();
  });
});

describe("each pattern's request", () => {
  const asked: Asked = {
    pattern: "snake",
    overlap: 0.15,
    stack: { below_um: 5, above_um: 5, step_um: 1 },
    positions: [],
  };
  const r = { x: 1_000, y: 2_000, width: 3_000, height: 4_000 };

  it("waits for a raster's, a snake's and a tiled stack's region", () => {
    for (const pattern of ["raster", "snake", "tiled_stack"]) {
      expect(requestOf({ ...asked, pattern }), pattern).toBeUndefined();
      expect(requestOf({ ...asked, pattern, region: r })?.region, pattern).toEqual(regionOf(r));
    }
    expect(requestOf({ ...asked, pattern: "tiled_stack", region: r })?.stack).toEqual(asked.stack);
    expect(requestOf({ ...asked, region: r })?.overlap).toBe(0.15);
  });

  it("waits for a z-scan's place, and positions' first", () => {
    expect(requestOf({ ...asked, pattern: "z_scan" })).toBeUndefined();
    expect(requestOf({ ...asked, pattern: "z_scan", at: { x: 1, y: 2 } })).toMatchObject({
      at: { x: 1, y: 2 },
      stack: asked.stack,
    });
    expect(requestOf({ ...asked, pattern: "positions" })).toBeUndefined();
    expect(
      requestOf({ ...asked, pattern: "positions", positions: [{ x: 1, y: 2 }] })?.positions,
    ).toEqual([{ x_um: 1, y_um: 2 }]);
  });

  it("asks for the tissue found as it is, and a workflow with its parameters", () => {
    expect(requestOf({ ...asked, pattern: "tissue" })).toEqual({
      pattern: "tissue",
      overlap: 0.15,
      positions: [],
    });
    expect(requestOf({ ...asked, pattern: "spiral", params: { turns: 3 } })?.params).toEqual({
      turns: 3,
    });
    expect(requestOf({ ...asked, pattern: "spiral" })?.params).toEqual({});
  });
});
