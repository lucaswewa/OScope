// An action that hands its work to a job (W3.2): the action run, its job
// followed to its end, and how it ended said. On a stand-in network.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";

import { standInNetwork } from "../../../tests/setup/stand-ins.ts";
import { useJobAction } from "./jobAction";

let states: string[];

const invocation = (status: string, output: unknown = null) => ({
  status,
  id: "an-invocation",
  action: "/api/v1/exposure/auto",
  href: "",
  timeStarted: null,
  timeRequested: null,
  timeCompleted: null,
  links: [],
  input: {},
  output,
  log: [],
  error: null,
});

const job = (state: string) => ({
  id: "a-job",
  kind: "exposure.auto",
  title: "Auto exposure",
  state,
  progress: { fraction: null, stage: "", counters: {}, eta_s: null },
  results: [],
  error: state === "failed" ? "it didn't converge in 10 steps" : null,
  submitted: "",
  started: null,
  finished: null,
  params: {},
  resource: "hardware",
  pausable: false,
  position: null,
});

beforeEach(() => {
  setActivePinia(createPinia());
  standInNetwork({
    "POST /api/v1/exposure/auto": () =>
      new Response(JSON.stringify(invocation("pending")), { status: 201 }),
    "GET /api/v1/action_invocations/an-invocation": () =>
      invocation("completed", { id: "a-job", href: "/api/v1/jobs/job/a-job" }),
    "GET /api/v1/jobs/job/a-job": () => job(states.shift() ?? "completed"),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("an action that hands its work to a job", () => {
  it("runs, follows its job to the end, and gives it", async () => {
    states = ["queued", "running", "completed"];
    const scope = effectScope();
    const auto = scope.run(() => useJobAction("/api/v1/exposure/auto"))!;
    const ended = auto.run({});
    expect(auto.running.value).toBe(true);
    expect((await ended)?.state).toBe("completed");
    expect([auto.running.value, auto.job.value?.state, auto.failed.value]).toEqual([
      false,
      "completed",
      undefined,
    ]);
    scope.stop();
  });

  it("says how a job that failed ended, in the job's words", async () => {
    states = ["running", "failed"];
    const scope = effectScope();
    const auto = scope.run(() => useJobAction("/api/v1/exposure/auto"))!;
    expect(await auto.run({})).toBeUndefined();
    expect(auto.failed.value).toBe("it didn't converge in 10 steps");
    scope.stop();
  });
});
