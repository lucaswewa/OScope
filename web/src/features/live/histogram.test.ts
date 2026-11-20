// The frame's values (W3.6): the histogram's bins and percentiles from a
// message recorded from the server (W1.5), the look's levels in the
// sensor's units, the saturation mask's bits a byte a pixel, and the
// colour maps.
import { describe, expect, it } from "vitest";

import { readMessage, type LiveStats } from "@/api/preview/message";
import recorded from "@/api/preview/recorded-messages.json";
import { unpacked } from "@/api/preview/renderer";
import { MAPS, sourceOf, tableOf } from "./colourMaps";
import { histogramSeries, percentilesOf, toLevel, toSensor } from "./histogram";

function recordedMessage() {
  const binary = atob(recorded.messages.stats);
  return readMessage(Uint8Array.from(binary, (c) => c.charCodeAt(0)).buffer);
}
const stats = recordedMessage().stats as LiveStats;

describe("the histogram, from a recorded message", () => {
  it("has a series a channel, each bin at its lower edge in the sensor's units", () => {
    const series = histogramSeries(stats, false, (channel) => channel);
    expect(series.map((one) => one.name)).toEqual(stats.channels);
    const width = (stats.full_scale + 1) / 16;
    expect(series[0]!.width).toBe(width);
    expect(Array.from(series[0]!.x)).toEqual(Array.from({ length: 16 }, (_, bin) => bin * width));
    expect(Array.from(series[0]!.y)).toEqual(stats.histogram[0]);
  });

  it("counts one more a bin when logarithmic, so that an empty one shows", () => {
    const series = histogramSeries(stats, true, (channel) => channel);
    expect(Array.from(series[1]!.y)).toEqual(stats.histogram[1]!.map((count) => count + 1));
  });

  it("names each channel's percentiles, in order", () => {
    const named = percentilesOf(stats, 0);
    expect(Object.keys(named)).toEqual(["0.1 %", "1 %", "50 %", "99 %", "99.9 %"]);
    expect(Object.values(named)).toEqual(stats.percentiles[0]);
    const values = Object.values(named);
    expect(values.every((value, i) => i === 0 || value >= values[i - 1]!)).toBe(true);
  });

  it("puts the look's levels on the sensor's axis, and back, within 0 and 1", () => {
    expect(toSensor(0.5, 4095)).toBe(2047.5);
    expect(toLevel(2047.5, 4095)).toBe(0.5);
    expect([toLevel(-10, 4095), toLevel(5000, 4095)]).toEqual([0, 1]);
  });
});

describe("the saturation mask", () => {
  it("is a byte a pixel, 255 where its bit is set, from each byte's highest bit down", () => {
    const mask = {
      width: 10,
      height: 2,
      stride: 2,
      bits: new Uint8Array([0b1000_0001, 0b0100_0000, 0, 0b1000_0000]),
    };
    expect(Array.from(unpacked(mask))).toEqual([
      255, 0, 0, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0,
    ]);
  });

  it("marks pixels in the recorded message, which had some saturated", () => {
    const { mask } = recordedMessage();
    expect(mask).toBeDefined();
    expect(unpacked(mask!).some((byte) => byte === 255)).toBe(true);
  });
});

describe("the colour maps", () => {
  it("are the image's own colours, then seven tables of 256 colours", () => {
    expect(tableOf("none")).toBeUndefined();
    for (const name of MAPS.filter((one) => one !== "none")) {
      expect(tableOf(name)).toHaveLength(256 * 3);
    }
  });

  it("go from dark to light where their published tables do", () => {
    const luminance = (table: Uint8Array, at: number) =>
      0.2126 * table[at * 3]! + 0.7152 * table[at * 3 + 1]! + 0.0722 * table[at * 3 + 2]!;
    for (const name of ["grey", "viridis", "magma", "inferno", "plasma"] as const) {
      const table = tableOf(name)!;
      expect(luminance(table, 255)).toBeGreaterThan(luminance(table, 0));
    }
    // viridis's first colour, as published: 0.267004, 0.004874, 0.329415.
    expect(Array.from(tableOf("viridis")!.slice(0, 3))).toEqual([68, 1, 84]);
  });

  it("show HiLo's ends: the darkest blue, the brightest red, grey between", () => {
    const hilo = tableOf("hilo")!;
    expect(Array.from(hilo.slice(0, 3))).toEqual([0, 0, 255]);
    expect(Array.from(hilo.slice(255 * 3))).toEqual([255, 0, 0]);
    expect(Array.from(hilo.slice(128 * 3, 129 * 3))).toEqual([128, 128, 128]);
  });

  it("name their sources", () => {
    expect(sourceOf("turbo")).toContain("Apache License 2.0");
    expect(sourceOf("magma")).toContain("CC0");
    expect(sourceOf("grey")).toBeUndefined();
  });
});
