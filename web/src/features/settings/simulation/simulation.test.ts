import { describe, expect, it } from "vitest";

import {
  formOf,
  pictureOf,
  settingsOf,
  slugOf,
  wrongIn,
  type SimulatorSettings,
} from "@/features/settings/simulation/simulation";

/** The server's own defaults, as `settings/simulator` gives them. */
const DEFAULTS: SimulatorSettings = {
  rate: 1,
  sensor: "full",
  objective: "10x",
  sample: "H&E tissue",
  focus: 5000,
  tilt: [0, 0],
  rotation: 0,
  mirrored: false,
  scale: 1,
  backlash: 2,
  latency: 5,
  encoders: true,
  lost_steps: [0, 5],
  vignetting: 0,
  response: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  samples: [{ name: "mine", kind: "image", path: "mine.png", um_per_pixel: 0.5 }],
  gpu: true,
};

describe("the simulator's settings' form", () => {
  it("shows each pair's halves apart, and writes them back as pairs", () => {
    const form = formOf({ ...DEFAULTS, tilt: [1.5, -2], lost_steps: [0.1, 7] });
    expect([form.tiltX, form.tiltY, form.lostChance, form.lostUm]).toEqual([1.5, -2, 0.1, 7]);
    const written = settingsOf({ ...form, tiltY: 3, lostUm: 4 }, DEFAULTS);
    expect([written.tilt, written.lost_steps]).toEqual([
      [1.5, 3],
      [0.1, 4],
    ]);
    expect(Object.keys(written)).not.toContain("tiltX");
  });

  it("keeps what it doesn't show: the rate, the colour response and one's own samples", () => {
    const written = settingsOf({ ...formOf(DEFAULTS), backlash: 3 }, DEFAULTS);
    expect(written).toEqual({ ...DEFAULTS, backlash: 3 });
    expect(settingsOf(formOf(DEFAULTS), DEFAULTS)).toEqual(DEFAULTS);
  });

  it("refuses what can't be: a scale of 0, a negative backlash, a chance past 1", () => {
    const form = formOf(DEFAULTS);
    expect(wrongIn(form)).toBeUndefined();
    expect(wrongIn({ ...form, scale: 0 })).toEqual({ field: "scale", why: "positive" });
    expect(wrongIn({ ...form, backlash: -1 })).toEqual({ field: "backlash", why: "notNegative" });
    expect(wrongIn({ ...form, vignetting: -0.1 })).toEqual({
      field: "vignetting",
      why: "notNegative",
    });
    expect(wrongIn({ ...form, lostChance: 1.5 })).toEqual({ field: "lostChance", why: "chance" });
    expect(wrongIn({ ...form, focus: Number.NaN })).toEqual({ field: "focus", why: "number" });
    // A turn either way, a tilt either way and a chance of 1 are all fine.
    expect(wrongIn({ ...form, rotation: -20, tiltX: -3, lostChance: 1 })).toBeUndefined();
  });
});

describe("the built-in samples", () => {
  it("each have a picture, by their names; one's own has none", () => {
    for (const name of [
      "H&E tissue",
      "H-DAB tissue",
      "test target",
      "reflective surface",
      "fluorescent cells",
      "sub-diffraction structures",
      "colour target",
    ]) {
      expect(pictureOf(name), name).toMatch(/\.jpg/);
    }
    expect(pictureOf("mine")).toBeUndefined();
    expect(slugOf("H&E tissue")).toBe("h-e-tissue");
  });
});
