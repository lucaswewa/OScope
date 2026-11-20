// The frame's histogram (W3.6), from P18's statistics (ADR-0035): a
// channel each, in the sensor's units, linear or logarithmic; the
// percentiles that mark it; and the look's black and white points between
// the sensor's values and the look's 0 to 1.
import type { LiveStats } from "@/api/preview/message";
import type { Series } from "@/ui/charts/draw";

/** The percentiles each channel's are, in order (ADR-0035). */
export const PERCENTILES = [0.1, 1, 50, 99, 99.9] as const;

/**
 * Each channel's histogram, its bins' lower edges in the sensor's units.
 * Logarithmic, a count is one more than it was: an empty bin is then 1,
 * which a logarithm can show.
 */
export function histogramSeries(
  stats: LiveStats,
  log: boolean,
  name: (channel: string) => string,
): Series[] {
  return stats.channels.map((channel, i) => {
    const counts = stats.histogram[i] ?? [];
    const width = (stats.full_scale + 1) / Math.max(1, counts.length);
    return {
      name: name(channel),
      kind: "histogram",
      x: counts.map((_, bin) => bin * width),
      y: log ? counts.map((count) => count + 1) : counts,
      width,
    };
  });
}

/** A channel's percentiles, by name, in the sensor's units. */
export function percentilesOf(stats: LiveStats, channel: number): Record<string, number> {
  const values = stats.percentiles[channel] ?? [];
  return Object.fromEntries(PERCENTILES.map((p, i) => [`${p} %`, values[i] ?? Number.NaN]));
}

/** A level of the look, from 0 to 1, in the sensor's units: what the histogram's axis counts. */
export function toSensor(level: number, fullScale: number): number {
  return level * fullScale;
}

/** A value in the sensor's units as a level of the look, kept from 0 to 1. */
export function toLevel(value: number, fullScale: number): number {
  return Math.min(1, Math.max(0, value / fullScale));
}
