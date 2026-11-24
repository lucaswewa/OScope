// The wizard's display, from states a server gave (scripts/record-wizards.mjs).
import { describe, expect, it } from "vitest";

import recorded from "@/features/settings/calibration/wizard/recorded-wizards.json";
import { setOf, shownOf, type Wizard } from "@/features/settings/calibration/wizard/wizard";

const states = recorded as unknown as Record<keyof typeof recorded, Wizard>;
const standings = (wizard: Wizard) => shownOf(wizard).steps.map((step) => step.standing);

describe("the wizard's display", () => {
  it("shows its first step in hand, to confirm or skip, the rest waiting", () => {
    const shown = shownOf(states.prompting);
    expect(standings(states.prompting)).toEqual([
      "prompting",
      "waiting",
      "waiting",
      "waiting",
      "waiting",
      "waiting",
    ]);
    expect(shown.step?.kind).toBe("objective");
    expect(shown.can).toEqual({ confirm: true, retry: false, skip: true, cancel: true });
    expect([shown.ended, shown.settled]).toEqual([undefined, 0]);
  });

  it("follows a running step's job, which can't be confirmed again or skipped", () => {
    const shown = shownOf(states.running);
    expect(shown.step?.standing).toBe("running");
    const { state } = states.running.steps[0]!;
    expect(shown.step?.job).toBe(state.state === "running" ? state.job : "a job");
    expect(shown.can).toEqual({ confirm: false, retry: false, skip: false, cancel: true });
  });

  it("says what each step found: the objective's verdict, and a set by its ID", () => {
    const shown = shownOf(states.geometryDone);
    const [objective, geometry] = shown.steps;
    expect(objective).toMatchObject({
      standing: "done",
      said: "plausible: the 10x",
      set: undefined,
    });
    expect(geometry!.set).toMatch(/^[0-9a-f-]{36}$/);
    expect(geometry!.said).toBeUndefined();
    expect([shown.step?.kind, shown.settled]).toEqual(["dark", 2]);
  });

  it("offers a failed step again, or to skip it, saying why it failed", () => {
    const shown = shownOf(states.flatFailed);
    expect(shown.step).toMatchObject({ kind: "flat", standing: "failed" });
    expect(shown.step?.said).toContain("isn't blank");
    expect(shown.can).toEqual({ confirm: false, retry: true, skip: true, cancel: true });
    expect(shownOf(states.retried).step).toMatchObject({ kind: "flat", standing: "prompting" });
    const skipped = shownOf(states.skipped);
    expect(skipped.steps[3]!.standing).toBe("skipped");
    expect([skipped.step?.kind, skipped.settled]).toEqual(["colour", 4]);
  });

  it("ends, cancelled or finished, with no step in hand and nothing to do", () => {
    const none = { confirm: false, retry: false, skip: false, cancel: false };
    const cancelled = shownOf(states.cancelled);
    expect([cancelled.ended, cancelled.step, cancelled.can]).toEqual([
      "cancelled",
      undefined,
      none,
    ]);
    // The step it was cancelled on still prompts, as the server left it.
    expect(cancelled.steps[4]!.standing).toBe("prompting");
    const finished = shownOf(states.finished);
    expect([finished.ended, finished.step, finished.can, finished.settled]).toEqual([
      "finished",
      undefined,
      none,
      1,
    ]);
  });

  it("takes a set from a result only as the server names one", () => {
    expect(setOf("calibration/01a20cc9-9c49-77fb-870d-cf5eda3282d9")).toBe(
      "01a20cc9-9c49-77fb-870d-cf5eda3282d9",
    );
    expect(setOf("plausible: the 10x")).toBeUndefined();
    expect(setOf("projects/a/acquisitions/01a20cc9-9c49-77fb-870d-cf5eda3282d9")).toBeUndefined();
  });
});
