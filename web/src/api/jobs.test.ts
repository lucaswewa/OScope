// Jobs (ADR-0016): followed at `jobs/job/{id}` to their end. The job
// service is stood in for here, with its recorded answers; the contract
// tests (tests/e2e/contract.spec.ts) follow real jobs.
import { afterEach, describe, expect, it, vi } from "vitest";

import { Server } from "./client.ts";
import { finished, JOB_ENDED, job, type Job, type JobState } from "./jobs.ts";
import recorded from "./recorded-answers.json";

const on = new Server("http://127.0.0.1:8211");
const done = recorded.job as unknown as Job;
const broken = recorded.failedJob as unknown as Job;
const ref = { id: done.id, href: `/api/v1/jobs/job/${done.id}` };

const at = (state: JobState, fraction: number | null): Job => ({
  ...done,
  state,
  progress: { ...done.progress, fraction },
  results: [],
  finished: null,
});

/** A job service whose job is, at each look, the next of `looks`, the last again and again. */
function serving(looks: Job[]): Request[] {
  const made: Request[] = [];
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    made.push(request);
    const now = looks[Math.min(made.length - 1, looks.length - 1)];
    return new Response(JSON.stringify(now), { headers: { "content-type": "application/json" } });
  });
  return made;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a job", () => {
  it("is followed to its end, each change told once: its results", async () => {
    serving([at("queued", null), at("running", 0.5), at("running", 0.5), done]);
    const told: string[] = [];
    const ended = await finished(ref, {
      on,
      seen: (now) => told.push(`${now.state} ${now.progress.fraction}`),
    });
    expect(ended.results).toEqual(done.results);
    expect(told).toEqual(["queued null", "running 0.5", "completed 1"]);
  });

  it("throws its failure in its own words", async () => {
    serving([broken]);
    await expect(finished(ref, { on })).rejects.toMatchObject({
      failure: "failed",
      message: "no acquisition 00000000-0000-0000-0000-000000000000",
    });
  });

  it("throws a cancellation, and an interruption as a failure", async () => {
    serving([{ ...done, state: "cancelled" }]);
    await expect(finished(ref, { on })).rejects.toMatchObject({ failure: "cancelled" });
    serving([{ ...done, state: "interrupted" }]);
    await expect(finished(ref, { on })).rejects.toMatchObject({
      failure: "failed",
      message: "the job ended interrupted",
    });
  });

  it("is no longer followed when its caller aborts, though it carries on", async () => {
    const made = serving([at("running", 0.1)]);
    const caller = new AbortController();
    const following = finished(ref.id, {
      on,
      signal: caller.signal,
      seen: () => caller.abort(),
    });
    await expect(following).rejects.toMatchObject({ failure: "aborted" });
    expect(made.map((request) => request.method)).toEqual(["GET"]);
  });

  it("is read at its address, by its id", async () => {
    const made = serving([done]);
    expect((await job(done.id, { on })).state).toBe("completed");
    expect(new URL(made[0]!.url).pathname).toBe(`/api/v1/jobs/job/${done.id}`);
  });

  it("doesn't leave the states that end it", () => {
    expect([...JOB_ENDED].sort()).toEqual(["cancelled", "completed", "failed", "interrupted"]);
  });
});
