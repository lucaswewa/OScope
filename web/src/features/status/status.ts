// The instrument's status (W2.2): `status/instrument`, read at first and
// every few seconds, and again at once when `status/changed` says it
// changed; and the tone each state is shown in. The server weighs the
// subsystems: the worst, as it counts them, is the instrument's state.
import { computed, type ComputedRef } from "vue";

import type { Server } from "@/api/client";
import type { ConnectionState } from "@/api/connection/connection";
import { useProperty } from "@/api/things/use";
import type { IconName } from "@/ui/icons";

/** A subsystem's or the instrument's state. */
export type InstrumentState = "ok" | "degraded" | "fault" | "offline";

/** A subsystem's health. */
export interface Health {
  subsystem: string;
  state: InstrumentState;
  message: string;
}

/** The instrument's status: its state, why, and its subsystems'. */
export interface Instrument {
  state: InstrumentState;
  message: string;
  subsystems: Health[];
}

/** How a state is shown: its tone, the colour the tokens give it. */
export type Tone = "success" | "warning" | "danger" | "neutral";

/** The tone of a state: a subsystem that isn't there is neutral. */
export function toneOf(state: InstrumentState | undefined): Tone {
  switch (state) {
    case "ok":
      return "success";
    case "degraded":
      return "warning";
    case "fault":
      return "danger";
    default:
      return "neutral";
  }
}

/**
 * Whether the page in place of the tabs shows: while the server isn't there
 * in a version the app knows; at the start, only once it is `late`.
 */
export function noServer(state: ConnectionState, late: boolean): boolean {
  return state !== "connected" && (state !== "connecting" || late);
}

/** The icon of a tone. */
export function iconOf(tone: Tone): IconName {
  return tone === "neutral" ? "info" : tone;
}

/**
 * The instrument's status as last read from the server at `on`. The Thing
 * store reads it again at once on each of the Thing's events, `changed`
 * among them: the property isn't observed (W1.2).
 */
export function useInstrument(on?: Server): ComputedRef<Instrument | undefined> {
  const read = useProperty("/api/v1/status/instrument", { every: 5_000, ...(on ? { on } : {}) });
  return computed(() => read.value.value as Instrument | undefined);
}
