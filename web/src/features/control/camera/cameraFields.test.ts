// The camera's fields (W3.2), from the simulated camera's recorded
// capabilities (scripts/record-capabilities.mjs): the ranges and steps, the modes
// and the formats each gives, the region's grid, the triggers; a format's
// key; and the settings that differ from those saved.
import { describe, expect, it } from "vitest";

import { cameraFields, differences, formatKey, stepOf, type Capabilities } from "./cameraFields";
import recorded from "./recorded-capabilities.json";

const caps = recorded as Capabilities;
const fields = cameraFields(caps);

describe("the camera's fields, from its capabilities", () => {
  it("take each setting's range, and a step to suit it", () => {
    expect(fields.exposure).toEqual({ min: 0.0184, max: 19_340.1, step: 1 });
    expect(fields.gain).toEqual({ min: 0, max: 240, step: 1 });
  });

  it("list the modes, and the formats each gives, the fastest first", () => {
    expect(fields.modes.map((mode) => mode.name)).toEqual(["full", "bin2", "bin3", "design"]);
    expect(fields.formats("full").map((one) => [one.value, one.fps])).toEqual([
      ["bayer8:RGGB", 14.49],
      ["bayer16:RGGB:12", 7.24],
    ]);
    expect(fields.formats("design").map((one) => [one.value, one.fps])).toEqual([
      ["bayer16:RGGB:12", 14.5],
    ]);
    expect(fields.formats("no such mode")).toEqual([]);
  });

  it("take the region's grid and the sensor's size, and the triggers the camera takes", () => {
    expect(fields.roi).toEqual({ xStep: 12, yStep: 12, minSize: 48, width: 5472, height: 3648 });
    expect(fields.triggers).toEqual(["free_running", "software", "external"]);
    expect(fields.temperature).toBe(false);
    // A camera with no software trigger and no region offers neither.
    const plain = cameraFields({
      ...caps,
      roi: null,
      trigger: { ...caps.trigger, software: false },
    });
    expect([plain.roi, plain.triggers]).toEqual([undefined, ["free_running", "external"]]);
  });
});

describe("a format's key", () => {
  it("tells apart each kind, pattern and depth", () => {
    const keys = [
      formatKey({ kind: "bayer16", pattern: "RGGB", significant_bits: 12 }),
      formatKey({ kind: "bayer16", pattern: "GRBG", significant_bits: 12 }),
      formatKey({ kind: "bayer16", pattern: "RGGB", significant_bits: 16 }),
      formatKey({ kind: "bayer8", pattern: "RGGB" }),
      formatKey({ kind: "mono16" }),
      formatKey({ kind: "planar_f32", planes: 3 }),
    ];
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("a range's step", () => {
  it("is the power of ten a hundred of which span it", () => {
    expect([stepOf(0, 240), stepOf(0, 1), stepOf(0, 5000), stepOf(5, 5)]).toEqual([1, 0.01, 10, 1]);
  });
});

describe("the settings that differ from those saved", () => {
  const now = {
    mode: "full",
    format: { kind: "bayer8", pattern: "RGGB" },
    exposure: 12.5,
    gain: 0,
    roi: null,
    trigger: { kind: "free_running" },
  };

  it("are none when the camera has its saved settings, or either isn't read", () => {
    expect(differences(now, { ...now })).toEqual([]);
    expect(differences(now, undefined)).toEqual([]);
    // A region that isn't there, given or not, is the same.
    const withoutRegion = { mode: now.mode, format: now.format, exposure: 12.5, gain: 0 };
    expect(differences(now, { ...withoutRegion, trigger: now.trigger })).toEqual([]);
  });

  it("are named, as auto exposure leaves them", () => {
    expect(differences(now, { ...now, exposure: 40 })).toEqual(["exposure"]);
    const changed = { ...now, mode: "design", trigger: { kind: "software" } };
    expect(differences(now, changed)).toEqual(["mode", "trigger"]);
  });
});
