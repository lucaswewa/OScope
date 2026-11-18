// Jobs (ADR-0016): the long pieces of work an action hands to the job
// service. The action answers at once with a `JobRef`, and the job is
// followed at `jobs/job/{id}`. That route's answer isn't in the OpenAPI
// document: its shape is written here, and checked against a real server by
// the contract tests (tests/e2e/contract.spec.ts).
import { call, pause, server, type Limits, type Server } from "./client.ts";
import { ApiError } from "./errors.ts";

/** Where to follow a job an action has started. */
export interface JobRef {
  id: string;
  /** Where it is, as it is now. */
  href: string;
}

/** Where a job is in its life. */
export type JobState =
  "queued" | "running" | "paused" | "completed" | "failed" | "cancelled" | "interrupted";

/** A job, as the job service tells it. */
export interface Job {
  id: string;
  /** Its kind: `capture.single`, `projects.export`… */
  kind: string;
  /** For people. */
  title: string;
  state: JobState;
  progress: {
    /** From 0 to 1, if known. */
    fraction: number | null;
    /** What it is doing. */
    stage: string;
    counters: Record<string, number>;
    /** How long it has left, in s, if known. */
    eta_s: number | null;
  };
  /** What it made: paths under the API, `projects/{project}/acquisitions/{id}`. */
  results: string[];
  /** Why it failed. */
  error: string | null;
  /** When it was submitted, started and finished. */
  submitted: string;
  started: string | null;
  finished: string | null;
  params: unknown;
  /** What it needs: `hardware`, or the computer's. */
  resource: string;
  pausable: boolean;
  /** Its place in the queue, if it waits there. */
  position: number | null;
}

/** The states a job doesn't leave. */
export const JOB_ENDED: ReadonlySet<JobState> = new Set([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

const PAUSE = { first: 100, growth: 1.5, most: 2000 };

/** A job as it is now. */
export async function job(
  id: string,
  options: Limits & { on?: Server | undefined } = {},
): Promise<Job> {
  const on = options.on ?? server;
  // Its answer has no schema in the OpenAPI document: see `Job`.
  const answer: unknown = await call(
    (signal) => on.api.GET("/api/v1/jobs/job/{id}", { params: { path: { id } }, signal }),
    options,
  );
  return answer as Job;
}

/** A job that didn't complete, as an ApiError. */
export function jobError(ended: Job): ApiError {
  if (ended.state === "cancelled") return new ApiError("cancelled", "the job was cancelled");
  return new ApiError("failed", ended.error ?? `the job ended ${ended.state}`, { body: ended });
}

/**
 * Follows a job to its end, telling `seen` each change of its state or
 * progress: the job once it has completed. Any other end is thrown as an
 * ApiError. Aborting stops the following, not the job.
 */
export async function finished(
  ref: JobRef | string,
  options: Limits & { on?: Server | undefined; seen?: (now: Job) => void } = {},
): Promise<Job> {
  const id = typeof ref === "string" ? ref : ref.id;
  const limits: Limits & { on?: Server | undefined } = { timeout: options.timeout, on: options.on };
  let wait = PAUSE.first;
  let last = "";
  for (;;) {
    const now = await job(id, options.signal ? { ...limits, signal: options.signal } : limits);
    const told = JSON.stringify([now.state, now.progress]);
    if (told !== last) options.seen?.(now);
    last = told;
    if (JOB_ENDED.has(now.state)) {
      if (now.state !== "completed") throw jobError(now);
      return now;
    }
    await pause(wait, options.signal);
    wait = Math.min(PAUSE.most, wait * PAUSE.growth);
  }
}
