// A scan as it runs, for its page (W5.2): its tiles done drawn on its
// plan, where it stands, when it should end, its fault and the tile it was
// at, and what it made. From the server's `scan/progress`, which the scan's
// job keeps, and the job itself.
import type { Input } from "@/api/actions";
import { JOB_ENDED, type Job, type JobState } from "@/api/jobs";
import type { ValueOf } from "@/api/things/use";
import { resultPlace } from "@/features/jobs/jobs";
import type { Rect } from "@/ui/tile-viewer/coordinates";
import type { Shape, ShapeKind } from "@/ui/tile-viewer/shapes";

import { frameOf, type Plan, type ScanRequest } from "./plan";

/** The latest scan's progress, as the server keeps it. */
export type Progress = NonNullable<ValueOf<"/api/v1/scan/progress">>;

/** A scan's plan's tiles in the order the scan takes them. */
function ordered(plan: Plan): Plan["tiles"] {
  return [...plan.tiles].sort((a, b) => a.index - b.index);
}

/** Why a scan failed, and the tile it was at, if its error names one. */
export interface Fault {
  why: string;
  /** The tile, by its index, and its place in the scan's order, from 1. */
  tile?: { index: number; number: number };
}

/**
 * A failed scan's fault: its error, and the tile the error names, as the
 * engine names each step's tile, "tile 5 of 12". A fault elsewhere, in its
 * focus map or once its tiles are taken, has no tile.
 */
export function faultOf(progress: Progress, job: Job | undefined): Fault | undefined {
  if (job?.state !== "failed") return undefined;
  const why = job.error ?? "";
  const named = /\btile (\d+) of (\d+)\b/.exec(why);
  const number = named ? Number(named[1]) : 0;
  const tile = ordered(progress.plan)[number - 1];
  return tile ? { why, tile: { index: tile.index, number } } : { why };
}

/** The job's state, or, until it is read, the progress's own. */
export function stateOf(progress: Progress, job: Job | undefined): JobState {
  if (job) return job.state;
  const state = progress.state as JobState;
  return JOB_ENDED.has(state) ? state : "running";
}

/** Whether the scan is still to end: queued, running or paused. */
export function going(progress: Progress, job: Job | undefined): boolean {
  return !JOB_ENDED.has(stateOf(progress, job));
}

/**
 * The scan drawn on its plan: each tile's frame, done, next while it goes,
 * at fault, or still to come; then the path from tile to tile.
 */
export function runShapes(progress: Progress, job: Job | undefined): Shape[] {
  const tiles = ordered(progress.plan);
  const done = new Set(progress.done.map((tile) => tile.index));
  const fault = faultOf(progress, job)?.tile?.index;
  const next = going(progress, job) ? tiles.find((tile) => !done.has(tile.index)) : undefined;
  const kind = (index: number): ShapeKind =>
    index === fault ? "fault" : done.has(index) ? "done" : index === next?.index ? "next" : "frame";
  const frames: Shape[] = tiles.map((tile) => ({
    kind: kind(tile.index),
    points: frameOf(progress.plan, tile),
  }));
  const path: Shape[] =
    tiles.length > 1
      ? [{ kind: "path", points: tiles.map((t) => ({ x: t.x_um, y: t.y_um })) }]
      : [];
  return [...frames, ...path];
}

/** Where a scan stands, for its pane. */
export interface Standing {
  state: JobState;
  /** Its tiles done, and in all. */
  done: number;
  total: number;
  /** When it should end, while it runs and the server can say. */
  endsAt?: Date;
  fault?: Fault;
  /** The acquisition it made, in the gallery's terms. */
  record?: { project: string; record: string };
  /** What a cancel or a fault left in scratch, as the job says it. */
  kept?: string;
  /** Autofocus runs refused, their tiles at the z last found. */
  refused: number;
}

/** Where the scan stands at `now`, in ms since the epoch. */
export function standingOf(progress: Progress, job: Job | undefined, now: number): Standing {
  const state = stateOf(progress, job);
  const eta = job?.progress.eta_s;
  const results = job?.results ?? [];
  const record = results.map(resultPlace).find((place) => place !== undefined);
  const kept = results.find((result) => result.startsWith("recording "));
  const fault = faultOf(progress, job);
  return {
    state,
    done: progress.done.length,
    total: progress.plan.tiles.length,
    ...(state === "running" && typeof eta === "number"
      ? { endsAt: new Date(now + eta * 1000) }
      : {}),
    ...(fault ? { fault } : {}),
    ...(record ? { record } : {}),
    ...(kept ? { kept } : {}),
    refused: progress.refused,
  };
}

/** What the run's pane asks for beside the plan. */
export interface Run {
  project: string;
  /** The acquisition's name: the server's own, "scan of 12 tiles", if blank. */
  name: string;
  /** How long the stage settles after a move before its frame, in ms. */
  settleMs: number;
}

type RunInput = Input<"/api/v1/scan/run">;

/**
 * The scan's run, of `request`, as `run` asks. Its options but the settling
 * are the server's defaults: the generated type asks for each, as their
 * defaults make it, though the server takes any left out.
 */
export function runInput(request: ScanRequest, run: Run): RunInput {
  const name = run.name.trim();
  const options = { settle_ms: run.settleMs } as NonNullable<RunInput["options"]>;
  return { request, project: run.project, ...(name ? { name } : {}), options };
}

/** Where the live mosaic is begun for a scan of `plan`: its frames' bounds, on the stage. */
export function mosaicRegionOf(plan: Plan): Rect | undefined {
  const corners = plan.tiles.flatMap((tile) => frameOf(plan, tile));
  if (corners.length === 0) return undefined;
  const xs = corners.map((p) => p.x);
  const ys = corners.map((p) => p.y);
  const [x, y] = [Math.min(...xs), Math.min(...ys)];
  return { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y };
}
