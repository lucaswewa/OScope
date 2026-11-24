// The calibration wizard's display (W8.3, P33): its steps, each where it
// stands and what it found, and what can be done with the step in hand.
// The wizard itself is the server's, kept across a restart: this is only
// how its state is shown.
import type { Schema } from "@/api/client";

export type Wizard = NonNullable<Schema<"calibration_wizard_value">>;
export type Step = Wizard["steps"][number];
export type Kind = Step["kind"];
export type Standing = Step["state"]["state"];

/** A step as it is shown. */
export interface ShownStep {
  kind: Kind;
  title: string;
  standing: Standing;
  /** Whether it is the step in hand. */
  current: boolean;
  /** Its job, while it runs. */
  job?: string | undefined;
  /** The set it made, by its ID. */
  set?: string | undefined;
  /** What it found, in words of the server's: an objective check's verdict, or why it failed. */
  said?: string | undefined;
}

/** What can be done now. */
export interface Can {
  confirm: boolean;
  retry: boolean;
  skip: boolean;
  cancel: boolean;
}

/** The wizard as it is shown. */
export interface Shown {
  steps: ShownStep[];
  /** The step in hand, if it isn't over. */
  step: ShownStep | undefined;
  can: Can;
  /** How it ended, if it did. */
  ended: "finished" | "cancelled" | undefined;
  /** How many steps are settled: done or skipped. */
  settled: number;
}

const SET = /^calibration\/([0-9a-f-]{36})$/;

/** The set a step's result names, by its ID: none for an objective check's verdict. */
export function setOf(result: string): string | undefined {
  return SET.exec(result)?.[1];
}

/** `wizard`, as it is shown. */
export function shownOf(wizard: Wizard): Shown {
  const over = wizard.finished || wizard.cancelled;
  const steps = wizard.steps.map((step, i): ShownStep => {
    const { state } = step;
    const result = state.state === "done" ? state.result : undefined;
    const set = result === undefined ? undefined : setOf(result);
    return {
      kind: step.kind,
      title: step.title,
      standing: state.state,
      current: !over && i === wizard.current,
      job: state.state === "running" ? state.job : undefined,
      set,
      said: state.state === "failed" ? state.error : set === undefined ? result : undefined,
    };
  });
  const step = steps.find((one) => one.current);
  const standing = step?.standing;
  const can: Can = {
    confirm: standing === "prompting",
    retry: standing === "failed",
    skip: standing === "prompting" || standing === "failed",
    cancel: !over,
  };
  const ended = wizard.cancelled ? "cancelled" : wizard.finished ? "finished" : undefined;
  const settled = steps.filter((one) => one.standing === "done" || one.standing === "skipped");
  return { steps, step, can, ended, settled: settled.length };
}
