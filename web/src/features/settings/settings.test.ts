// The Settings tab's arithmetic (W8.1): what changed in a draft, the
// settings document's file name and what a file read back holds, and a
// camera's saved settings in words.
import { describe, expect, it } from "vitest";

import { cameraSaid, changedParts, documentName, documentOf } from "./settings";

const system = { data_root: "data", scratch_root: "scratch", address: "127.0.0.1", port: 8210 };

describe("the settings", () => {
  it("name the parts of a draft that differ, in the saved order", () => {
    expect(changedParts(system, { ...system })).toEqual([]);
    expect(changedParts(system, { ...system, port: 8211, data_root: "d" })).toEqual([
      "data_root",
      "port",
    ]);
  });

  it("export to a file named by its day", () => {
    expect(documentName(new Date(2026, 10, 3, 23, 59))).toBe("oscope-settings-2026-11-03.json");
  });

  it("read back a settings document, and say why a file isn't one", () => {
    const document = { format: "oscope.settings", version: 1, system };
    expect(documentOf(JSON.stringify(document))).toEqual({ document });
    expect(documentOf("{ not json")).toEqual({ why: "notJson" });
    expect(documentOf(JSON.stringify({ format: "something else" }))).toEqual({
      why: "notSettings",
    });
    expect(documentOf("[1, 2]")).toEqual({ why: "notSettings" });
    expect(documentOf("null")).toEqual({ why: "notSettings" });
  });

  it("say a camera's saved settings in words", () => {
    const words = { all: "all of the sensor", trigger: (kind: string) => `by ${kind}` };
    const saved = {
      mode: "bin3",
      format: { kind: "bayer16" as const, pattern: "RGGB" as const, significant_bits: 12 },
      exposure: 24.398168,
      gain: 3,
      roi: { x: 12, y: 24, width: 960, height: 648 },
      trigger: { kind: "external" as const, edge: "rising" as const },
    };
    expect(cameraSaid(saved, words)).toEqual([
      { term: "mode", value: "bin3" },
      { term: "format", value: "bayer16" },
      { term: "exposure", value: "24.4 ms" },
      { term: "gain", value: "3" },
      { term: "roi", value: "12, 24: 960 × 648" },
      { term: "trigger", value: "by external, rising" },
    ]);
    const whole = { ...saved, roi: null, trigger: { kind: "free_running" as const } };
    expect(cameraSaid(whole, words).slice(4)).toEqual([
      { term: "roi", value: "all of the sensor" },
      { term: "trigger", value: "by free_running" },
    ]);
  });
});
