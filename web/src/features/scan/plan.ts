// A scan's plan, for its page (W5.1): its tiles as frames on the stage, in
// order, the path between them, what it would take, and what is wrong with
// it before anything moves.
import type { Input, Output } from "@/api/actions";
import type { ValueOf } from "@/api/things/use";
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";
import { rectShape, type Shape } from "@/ui/tile-viewer/shapes";

/** The server's plan of a scan, and its estimate. */
export type Preview = Output<"/api/v1/scan/plan">;
export type Plan = Preview["plan"];
export type Estimate = Preview["estimate"];
export type Travel = ValueOf<"/api/v1/stage/capabilities">["x"];

/** A tile's frame on the stage: its field's corners about the tile's centre. */
export function frameOf(plan: Plan, tile: Plan["tiles"][number]): Point[] {
  return plan.field.corners_um.map((corner) => ({
    x: tile.x_um + corner.x,
    y: tile.y_um + corner.y,
  }));
}

/**
 * The plan drawn: each tile's frame, the first marked, then the path from
 * tile to tile in their order, and the region it covers.
 */
export function shapesOf(plan: Plan, region?: Rect): Shape[] {
  const tiles = [...plan.tiles].sort((a, b) => a.index - b.index);
  const frames: Shape[] = tiles.map((tile, i) => ({
    kind: i === 0 ? "first" : "frame",
    points: frameOf(plan, tile),
  }));
  const path: Shape[] =
    tiles.length > 1
      ? [{ kind: "path", points: tiles.map((t) => ({ x: t.x_um, y: t.y_um })) }]
      : [];
  return [...frames, ...path, ...(region ? [rectShape("region", region)] : [])];
}

/** What is wrong with a plan, before anything moves. */
export type Warning =
  | { kind: "limits"; tiles: number }
  | { kind: "calibration" }
  | { kind: "room"; needed: number; available: number };

/** What a plan's warnings are worked out from: the stage's travel, the calibration and the disk. */
export interface Circumstances {
  travel?: { x: Travel; y: Travel } | undefined;
  /** Whether the fitted objective has an active geometry, at the camera's binning. */
  calibrated?: boolean | undefined;
  /** The scratch disk's space OScope can still use, in bytes. */
  available?: number | undefined;
}

/**
 * Its warnings: tiles whose frame reaches past the stage's travel, a field
 * spaced by the objective's nominal pixel for want of a calibration, and a
 * recording larger than the scratch disk holds. Each only when it is known.
 */
export function warningsOf(preview: Preview, now: Circumstances): Warning[] {
  const warnings: Warning[] = [];
  const travel = now.travel;
  if (travel) {
    const within = (p: Point) =>
      p.x >= travel.x.min && p.x <= travel.x.max && p.y >= travel.y.min && p.y <= travel.y.max;
    const past = preview.plan.tiles.filter((tile) => !frameOf(preview.plan, tile).every(within));
    if (past.length > 0) warnings.push({ kind: "limits", tiles: past.length });
  }
  if (now.calibrated === false) warnings.push({ kind: "calibration" });
  if (now.available !== undefined && preview.estimate.bytes > now.available) {
    warnings.push({ kind: "room", needed: preview.estimate.bytes, available: now.available });
  }
  return warnings;
}

/** The frames an overview of `region` takes, a field apart, with frames `field` µm across. */
export function overviewFrames(region: Rect, field: { width: number; height: number }): number {
  if (!(field.width > 0 && field.height > 0)) return 0;
  return Math.ceil(region.width / field.width) * Math.ceil(region.height / field.height);
}

/** A rectangle typed in mm, as one of the stage in µm; undefined unless all four are numbers and it encloses something. */
export function rectFromMm(
  x?: number | null,
  y?: number | null,
  width?: number | null,
  height?: number | null,
): Rect | undefined {
  const all = [x, y, width, height];
  if (!all.every((v) => typeof v === "number" && Number.isFinite(v))) return undefined;
  if (!(width! > 0 && height! > 0)) return undefined;
  return { x: x! * 1000, y: y! * 1000, width: width! * 1000, height: height! * 1000 };
}

/** A rectangle as the server's region takes it. */
export function regionOf(rect: Rect) {
  return {
    rect: { origin: { x: rect.x, y: rect.y }, size: { width: rect.width, height: rect.height } },
  };
}

/**
 * The coverslip of a standard slide at the stage's origin, 50 × 24 mm from
 * (22, 0.5) mm, past its label: where an overview starts until the user
 * says otherwise.
 */
export const COVERSLIP: Rect = { x: 22_000, y: 500, width: 50_000, height: 24_000 };

/** A scan as the server's `plan` and `run` take it. */
export type ScanRequest = Input<"/api/v1/scan/plan">["request"];
export type Stack = NonNullable<ScanRequest["stack"]>;
export type ZPolicy = NonNullable<ScanRequest["z"]>;

/**
 * How each tile's z is found, as the run's pane chooses it (W5.2): at the
 * stage's z now; by autofocus every so many tiles, those between at the z
 * last found; or by a focus map measured before the first tile, at a grid's
 * points, a plane or a thin-plate spline fitted.
 */
export type Focus =
  | { kind: "here" }
  | { kind: "autofocus"; every: number }
  | { kind: "map"; columns: number; rows: number; surface: "plane" | "thin_plate" };

/**
 * The request's z for `focus`: none for the stage's z now, the server's own
 * default. The tissue found's default is the server's too: a map of 3 × 3
 * points, a plane, as its run's pane says.
 */
export function zOf(focus: Focus): ZPolicy | undefined {
  switch (focus.kind) {
    case "here":
      return undefined;
    case "autofocus":
      return { kind: "autofocus", every: focus.every };
    case "map": {
      const surface =
        focus.surface === "plane"
          ? { kind: "plane" as const }
          : { kind: "thin_plate" as const, smoothing: 0 };
      return { kind: "measure", columns: focus.columns, rows: focus.rows, surface };
    }
  }
}

/** What the plan's form has: its pattern, and what each pattern may need. */
export interface Asked {
  pattern: string;
  /** The share of a frame its neighbours overlap. */
  overlap: number;
  region?: Rect | undefined;
  stack: Stack;
  /** Where a z-scan is taken. */
  at?: Point | undefined;
  positions: readonly Point[];
  /** A workflow's parameters, by its schema. */
  params?: unknown;
  /** How each tile's z is found: at the stage's z now, if not said. */
  focus?: Focus | undefined;
}

/** The built-in patterns: any other is an extension's workflow. */
const BUILT_IN = ["raster", "snake", "tiled_stack", "z_scan", "positions", "tissue"];

/**
 * The request the form asks for: none while what its pattern needs is
 * missing, a region for a raster, a snake or a tiled stack, a place for a
 * z-scan, a position for positions. The tissue's region is the last
 * overview's, and a workflow's its parameters'.
 */
export function requestOf(asked: Asked): ScanRequest | undefined {
  const z = asked.focus && zOf(asked.focus);
  const base: ScanRequest = {
    pattern: asked.pattern,
    overlap: asked.overlap,
    positions: [],
    ...(z ? { z } : {}),
  };
  const region = asked.region && regionOf(asked.region);
  switch (asked.pattern) {
    case "raster":
    case "snake":
      return region && { ...base, region };
    case "tiled_stack":
      return region && { ...base, region, stack: asked.stack };
    case "z_scan":
      return asked.at && { ...base, at: asked.at, stack: asked.stack };
    case "positions":
      return asked.positions.length > 0
        ? { ...base, positions: asked.positions.map((p) => ({ x_um: p.x, y_um: p.y })) }
        : undefined;
    case "tissue":
      return base;
    default:
      return BUILT_IN.includes(asked.pattern) ? undefined : { ...base, params: asked.params ?? {} };
  }
}
