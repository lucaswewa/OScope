import { describe, expect, it } from "vitest";

import type { Job } from "@/api/jobs";

import {
  historyLimit,
  inOrder,
  mayHaveOlder,
  movedTo,
  percentOf,
  ranFor,
  resultPlace,
  timeLeft,
} from "./jobs";

/** A job, as the server tells one, with what a test changes. */
function aJob(changes: Partial<Job> = {}): Job {
  return {
    id: "j",
    kind: "capture.burst",
    title: "A burst",
    state: "queued",
    progress: { fraction: null, stage: "", counters: {}, eta_s: null },
    results: [],
    error: null,
    submitted: "2026-11-20T10:00:00Z",
    started: null,
    finished: null,
    params: {},
    resource: "hardware",
    pausable: false,
    position: null,
    ...changes,
  };
}

describe("a job's progress", () => {
  it("is a share from 0 to 100 when the job says, and nothing when it doesn't", () => {
    const at = (fraction: number | null) =>
      percentOf(aJob({ progress: { fraction, stage: "", counters: {}, eta_s: null } }));
    expect([at(0), at(0.426), at(1), at(1.2), at(-0.1), at(null)]).toEqual([
      0,
      43,
      100,
      100,
      0,
      undefined,
    ]);
  });

  it("says the time left in minutes and seconds, from the server's estimate", () => {
    expect(timeLeft(0)).toEqual({ minutes: 0, seconds: 0 });
    expect(timeLeft(59.6)).toEqual({ minutes: 1, seconds: 0 });
    expect(timeLeft(125)).toEqual({ minutes: 2, seconds: 5 });
    expect([timeLeft(null), timeLeft(undefined), timeLeft(-3), timeLeft(Number.NaN)]).toEqual([
      undefined,
      undefined,
      undefined,
      undefined,
    ]);
  });

  it("has run from its start to its end, or to now", () => {
    const started = "2026-11-20T10:00:00Z";
    const now = Date.parse("2026-11-20T10:01:30Z");
    expect(ranFor(aJob(), now)).toBeUndefined();
    expect(ranFor(aJob({ started }), now)).toBe(90);
    expect(ranFor(aJob({ started, finished: "2026-11-20T10:00:12.5Z" }), now)).toBe(12.5);
  });
});

describe("the queue", () => {
  it("runs first, then what is paused, then what waits, by its place", () => {
    const queue = [
      aJob({ id: "c", position: 1 }),
      aJob({ id: "p", state: "paused", resource: "compute" }),
      aJob({ id: "a", position: 0 }),
      aJob({ id: "r", state: "running" }),
    ];
    expect(inOrder(queue).map((job) => job.id)).toEqual(["r", "p", "a", "c"]);
  });

  it("moves a waiting job by Alt and the arrows, Home and End, within its ends", () => {
    const alt = (key: string) => ({ key, altKey: true });
    expect(movedTo(alt("ArrowUp"), 2, 4)).toBe(1);
    expect(movedTo(alt("ArrowDown"), 2, 4)).toBe(3);
    expect(movedTo(alt("Home"), 2, 4)).toBe(0);
    expect(movedTo(alt("End"), 2, 4)).toBe(3);
    // Past the ends, already there, without Alt, or another key: nowhere.
    expect(movedTo(alt("ArrowUp"), 0, 4)).toBeUndefined();
    expect(movedTo(alt("ArrowDown"), 3, 4)).toBeUndefined();
    expect(movedTo(alt("End"), 3, 4)).toBeUndefined();
    expect(movedTo({ key: "ArrowUp", altKey: false }, 2, 4)).toBeUndefined();
    expect(movedTo(alt("x"), 2, 4)).toBeUndefined();
  });
});

describe("the history", () => {
  it("asks for a page more each time, up to the server's thousand", () => {
    expect([1, 2, 3, 40, 41].map((pages) => historyLimit(pages, 25))).toEqual([
      25, 50, 75, 1000, 1000,
    ]);
  });

  it("may have older jobs while each answer is as long as asked", () => {
    expect(mayHaveOlder(25, 25)).toBe(true);
    expect(mayHaveOlder(24, 25)).toBe(false);
    expect(mayHaveOlder(1000, 1000)).toBe(false);
  });

  it("leads a project's acquisitions and results to the gallery, and nothing else", () => {
    expect(resultPlace("projects/p1/acquisitions/a2")).toEqual({ project: "p1", record: "a2" });
    expect(resultPlace("projects/p1/results/r3")).toEqual({ project: "p1", record: "r3" });
    expect(resultPlace("data/p1/acquisitions/a2/rendition.png")).toBeUndefined();
    expect(resultPlace("projects/p1/acquisitions/a2/thumbnail")).toBeUndefined();
  });
});
