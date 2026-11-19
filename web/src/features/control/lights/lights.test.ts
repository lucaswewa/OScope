// The lights' list (W3.4), from the simulated lights' recorded capabilities
// (scripts/record-capabilities.mjs), and the channels not as saved.
import { describe, expect, it } from "vitest";

import { lightRows, unsaved, type Channel, type Light } from "./lights";
import recorded from "./recorded-capabilities.json";

const channels = (recorded as { channels: Channel[] }).channels;
const light = (channel: string, on: boolean, intensity: number): Light => ({
  channel,
  on,
  intensity,
});

describe("the lights' list, from their capabilities", () => {
  it("has each channel, in their order, as it is now", () => {
    const rows = lightRows(channels, [
      light("transmitted", true, 0.4),
      light("blue 488", false, 1),
    ]);
    expect(rows.map((row) => [row.name, row.kind, row.dimmable, row.on, row.intensity])).toEqual([
      ["transmitted", "transmitted", true, true, 0.4],
      ["reflected", "reflected", true, false, 0],
      ["violet 405", "other", true, false, 0],
      ["blue 488", "other", true, false, 1],
      ["green 561", "other", true, false, 0],
      ["red 640", "other", true, false, 0],
    ]);
  });

  it("has every channel off until the lights' state is told", () => {
    const rows = lightRows(channels, undefined);
    expect(rows).toHaveLength(6);
    expect(rows.every((row) => !row.on && row.intensity === 0)).toBe(true);
  });
});

describe("the lights not as saved", () => {
  const saved = [light("transmitted", true, 1), light("reflected", false, 1)];

  it("are none while they are as saved, or either is untold", () => {
    expect(unsaved([light("transmitted", true, 1), light("reflected", false, 1)], saved)).toEqual(
      [],
    );
    expect(unsaved(undefined, saved)).toEqual([]);
    expect(unsaved(saved, undefined)).toEqual([]);
  });

  it("are the saved states of those switched, dimmed or gone since", () => {
    expect(unsaved([light("transmitted", true, 0.5), light("reflected", true, 1)], saved)).toEqual(
      saved,
    );
    expect(unsaved([light("transmitted", true, 1)], saved)).toEqual([light("reflected", false, 1)]);
  });
});
