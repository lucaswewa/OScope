// A scan as it runs (W5.2): its progress drawn on its plan and where it
// stands, from a scan's messages recorded on the simulator
// (scripts/record-scan.mjs): a tile done, paused, completed, and another
// cancelled; a fault, with the tile its error names; and the run's input.
import { describe, expect, it } from "vitest";

import type { Job } from "@/api/jobs";

import { frameOf, zOf } from "./plan";
import recorded from "./recorded-scan.json";
import { faultOf, mosaicRegionOf, runInput, runShapes, standingOf, type Progress } from "./running";

type Moment = { progress: Progress; job: Job };
const { running, paused, completed, cancelled } = recorded as unknown as Record<string, Moment>;

const kinds = (moment: Moment, job: Job | undefined = moment.job) =>
  runShapes(moment.progress, job).map((shape) => shape.kind);
const count = (all: string[], kind: string) => all.filter((one) => one === kind).length;
/** The tiles in the order the scan takes them. */
const ordered = (progress: Progress) => [...progress.plan.tiles].sort((a, b) => a.index - b.index);
const failing = (error: string): Job => ({ ...running!.job, state: "failed", error });

describe("a scan's progress on its plan", () => {
  it("marks each tile done, and the next while it runs, the path after", () => {
    const all = kinds(running!);
    const { done, plan } = running!.progress;
    expect(all).toHaveLength(plan.tiles.length + 1);
    expect(count(all, "done")).toBe(done.length);
    expect(count(all, "next")).toBe(1);
    expect(all.at(-1)).toBe("path");
    // The next is the first tile, in the scan's order, not yet done.
    const next = ordered(running!.progress).find(
      (tile) => !done.some((one) => one.index === tile.index),
    )!;
    const shapes = runShapes(running!.progress, running!.job);
    expect(shapes.find((shape) => shape.kind === "next")!.points).toEqual(frameOf(plan, next));
  });

  it("keeps its next tile marked while paused, and none once ended", () => {
    expect(paused!.job.state).toBe("paused");
    expect(count(kinds(paused!), "next")).toBe(1);
    expect(count(kinds(completed!), "next")).toBe(0);
    expect(count(kinds(completed!), "done")).toBe(completed!.progress.plan.tiles.length);
    expect(count(kinds(cancelled!), "next")).toBe(0);
  });

  it("marks a fault's tile, as its error names it, in the scan's order", () => {
    const tiles = ordered(running!.progress);
    const job = failing(`the move to plane 1 of tile 3 of ${tiles.length} failed 3 times: alarm`);
    expect(faultOf(running!.progress, job)).toEqual({
      why: job.error,
      tile: { index: tiles[2]!.index, number: 3 },
    });
    const shapes = runShapes(running!.progress, job);
    const fault = shapes.filter((shape) => shape.kind === "fault");
    expect(fault.map((shape) => shape.points)).toEqual([
      frameOf(running!.progress.plan, tiles[2]!),
    ]);
    expect(count(kinds(running!, job), "next")).toBe(0);
    // A fault in the focus map, before any tile, has none.
    expect(faultOf(running!.progress, failing("the focus map: no focus found"))).toEqual({
      why: "the focus map: no focus found",
    });
    expect(faultOf(running!.progress, running!.job)).toBeUndefined();
  });
});

describe("where a scan stands", () => {
  const now = Date.parse("2026-11-21T10:00:00Z");

  it("counts its tiles, and ends when the server says, while it runs", () => {
    const job = { ...running!.job, progress: { ...running!.job.progress, eta_s: 12 } };
    const standing = standingOf(running!.progress, job, now);
    expect(standing).toMatchObject({
      state: "running",
      done: running!.progress.done.length,
      total: running!.progress.plan.tiles.length,
      refused: 0,
    });
    expect(standing.endsAt).toEqual(new Date(now + 12_000));
    const held = { ...paused!.job, progress: { ...paused!.job.progress, eta_s: 12 } };
    expect(standingOf(paused!.progress, held, now).endsAt).toBeUndefined();
  });

  it("says the acquisition a scan made, and what a cancel kept in scratch", () => {
    const made = standingOf(completed!.progress, completed!.job, now);
    expect(made.state).toBe("completed");
    expect(made.record?.project).toMatch(/\S/);
    expect(completed!.job.results).toContain(
      `projects/${made.record!.project}/acquisitions/${made.record!.record}`,
    );
    const stopped = standingOf(cancelled!.progress, cancelled!.job, now);
    expect(stopped.state).toBe("cancelled");
    expect(stopped.record).toBeUndefined();
    expect(stopped.kept).toMatch(/^recording \S+, kept in scratch$/);
    expect(stopped.done).toBeGreaterThan(0);
    expect(stopped.done).toBeLessThan(stopped.total);
  });

  it("is the progress's own until its job is read", () => {
    expect(standingOf(running!.progress, undefined, now).state).toBe("running");
    expect(standingOf(completed!.progress, undefined, now).state).toBe("completed");
  });
});

describe("a scan's run", () => {
  const request = { pattern: "snake", overlap: 0.1, positions: [] };

  it("asks for the plan's request, the project, the name if given, and the settling", () => {
    expect(runInput(request, { project: "p", name: "  ", settleMs: 80 })).toEqual({
      request,
      project: "p",
      options: { settle_ms: 80 },
    });
    expect(runInput(request, { project: "p", name: " slide 4 ", settleMs: 0 })).toMatchObject({
      name: "slide 4",
      options: { settle_ms: 0 },
    });
  });

  it("finds each tile's z as chosen", () => {
    expect(zOf({ kind: "here" })).toBeUndefined();
    expect(zOf({ kind: "autofocus", every: 4 })).toEqual({ kind: "autofocus", every: 4 });
    expect(zOf({ kind: "map", columns: 4, rows: 3, surface: "thin_plate" })).toEqual({
      kind: "measure",
      columns: 4,
      rows: 3,
      surface: { kind: "thin_plate", smoothing: 0 },
    });
  });

  it("begins the live mosaic over every frame of the plan", () => {
    const { plan } = completed!.progress;
    const region = mosaicRegionOf(plan)!;
    const corners = plan.tiles.flatMap((tile) => frameOf(plan, tile));
    for (const corner of corners) {
      expect(corner.x).toBeGreaterThanOrEqual(region.x - 1e-9);
      expect(corner.x).toBeLessThanOrEqual(region.x + region.width + 1e-9);
      expect(corner.y).toBeGreaterThanOrEqual(region.y - 1e-9);
      expect(corner.y).toBeLessThanOrEqual(region.y + region.height + 1e-9);
    }
    expect(Math.min(...corners.map((c) => c.x))).toBeCloseTo(region.x);
    expect(Math.max(...corners.map((c) => c.y))).toBeCloseTo(region.y + region.height);
  });
});
