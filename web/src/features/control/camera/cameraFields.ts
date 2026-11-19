// The camera's fields (W3.2), from what the camera says it can do: each
// setting's range and step, its modes and the formats each gives, the
// region's grid and the triggers it takes; and which settings differ from
// those saved. The camera rounds a region to its own rules. Words are the
// pane's to add.
import type { ValueOf } from "@/api/things/use";

/** What the camera says it can do. */
export type Capabilities = ValueOf<"/api/v1/camera/capabilities">;
/** A mode, as the camera lists it. */
export type Mode = Capabilities["modes"][number];
/** A pixel format. */
export type Format = Mode["rates"][number]["format"];
/** What starts each exposure. */
export type Trigger = ValueOf<"/api/v1/camera/trigger">;
/** A region of the sensor, in its pixels. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A number's range, and the step its arrows move it by. */
export interface Range {
  min: number;
  max: number;
  step: number;
}

/** A format a mode gives, and its rate. */
export interface FormatChoice {
  /** Its key, for a list. */
  value: string;
  format: Format;
  fps: number;
}

/** The camera's fields. */
export interface CameraFields {
  exposure: Range;
  gain: Range;
  modes: Mode[];
  /** The formats `mode` gives, the fastest first. */
  formats(mode: string): FormatChoice[];
  /** The region's grid, the sensor's size, and its smallest side: none if it takes no region. */
  roi: { xStep: number; yStep: number; minSize: number; width: number; height: number } | undefined;
  /** The triggers it takes. */
  triggers: Trigger["kind"][];
  /** Whether it says its temperature. */
  temperature: boolean;
}

/** A format's key: its kind, with its pattern and bits where it has them. */
export function formatKey(format: Format): string {
  const parts: (string | number)[] = [format.kind];
  if ("pattern" in format) parts.push(format.pattern);
  if ("significant_bits" in format) parts.push(format.significant_bits);
  if ("planes" in format) parts.push(format.planes);
  return parts.join(":");
}

/** The step of a range a hundred of which span it: 1, 0.1, 10… */
export function stepOf(min: number, max: number): number {
  const span = max - min;
  if (!(span > 0)) return 1;
  return 10 ** Math.floor(Math.log10(span / 100));
}

/** The fields, from the camera's capabilities. */
export function cameraFields(caps: Capabilities): CameraFields {
  const range = (min: number, max: number): Range => ({ min, max, step: stepOf(min, max) });
  const roi = caps.roi ?? undefined;
  return {
    // A millisecond: the exposures a microscope uses are some to some hundreds.
    exposure: { min: caps.exposure_min, max: caps.exposure_max, step: 1 },
    gain: range(caps.gain_min, caps.gain_max),
    modes: caps.modes,
    formats(mode: string) {
      const rates = caps.modes.find((one) => one.name === mode)?.rates ?? [];
      return rates
        .map((rate) => ({ value: formatKey(rate.format), format: rate.format, fps: rate.fps }))
        .sort((a, b) => b.fps - a.fps);
    },
    roi: roi && {
      xStep: roi.x_step,
      yStep: roi.y_step,
      minSize: roi.min_size,
      width: caps.sensor_width,
      height: caps.sensor_height,
    },
    triggers: [
      "free_running",
      ...(caps.trigger.software ? (["software"] as const) : []),
      ...(caps.trigger.external ? (["external"] as const) : []),
    ],
    temperature: caps.temperature,
  };
}

/** The settings, as the camera has them and as they were saved: what differs, by name. */
export function differences(
  now: Record<string, unknown> | undefined,
  saved: Record<string, unknown> | undefined,
): string[] {
  if (!now || !saved) return [];
  const names = ["mode", "format", "exposure", "gain", "roi", "trigger"];
  return names.filter(
    (name) => JSON.stringify(now[name] ?? null) !== JSON.stringify(saved[name] ?? null),
  );
}
