// The lights (W3.4): each channel the lights' `capabilities` give, as it is
// now, and those that differ from the saved state.
import type { Schema } from "@/api/client";

export type Channel = Schema<"illumination_capabilities_value">["channels"][number];
export type Light = Schema<"illumination_lights_value">[number];

/** A channel, and how it is now. */
export interface LightRow extends Channel {
  on: boolean;
  /** From 0 to 1. */
  intensity: number;
}

/** Each channel, in the order the capabilities give them: off until its state is told. */
export function lightRows(
  channels: readonly Channel[],
  now: readonly Light[] | undefined,
): LightRow[] {
  return channels.map((channel) => {
    const light = now?.find((one) => one.channel === channel.name);
    return { ...channel, on: light?.on ?? false, intensity: light?.intensity ?? 0 };
  });
}

/** The saved state of each channel that is not as saved now: what putting them back sets. */
export function unsaved(
  now: readonly Light[] | undefined,
  saved: readonly Light[] | undefined,
): Light[] {
  if (!now || !saved) return [];
  return saved.filter((was) => {
    const light = now.find((one) => one.channel === was.channel);
    return !light || light.on !== was.on || Math.abs(light.intensity - was.intensity) > 1e-6;
  });
}
