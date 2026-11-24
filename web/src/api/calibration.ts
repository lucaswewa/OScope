// The calibration's sets (P32, P33, W8.2): what each kind measured, which
// the OpenAPI document leaves open, typed here from P33's `Values`; and a
// dark frame's or a flat field's image, from the endpoint it doesn't describe.
import { fetched, server, type Limits, type Server } from "./client.ts";

/** An objective's geometry at a binning. */
export interface Geometry {
  kind: "geometry";
  objective: string;
  binning: number;
  nominal_pixel_um: number;
  pixel_um: number;
  rotation_deg: number;
  mirrored: boolean;
  /** The fit's root-mean-square error, in pixels. */
  residual_px: number;
  /** Each move's, x and y, in pixels: none in a set made before W8.2. */
  residuals?: [number, number][];
  moves: number;
  step_um: number;
}

/** The stage's backlash. */
export interface Stage {
  kind: "stage";
  backlash_um: { x: number; y: number };
}

/** A dark frame, for a mode, a gain and an octave of exposure. */
export interface Dark {
  kind: "dark";
  mode: string;
  gain: number;
  taken_ms: number;
  frames: number;
  /** Its mean and its pixels' spread, from 0 to full scale's 1. */
  mean: number;
  spread: number;
}

/** A flat field, for an objective, a light and a binning. */
export interface Flat {
  kind: "flat";
  objective: string;
  light: string;
  binning: number;
  frames: number;
  /** Whether a dark frame was taken off it. */
  dark: boolean;
  gains: number[];
  /** How uneven it is: its spread over its mean. */
  uneven: number;
}

/** A colour calibration, for a light. */
export interface Colour {
  kind: "colour";
  light: string;
  gains: number[];
  /** By rows, applied after the gains. */
  matrix: number[][];
  patches: number;
  delta_e_before: number;
  delta_e_mean: number;
  delta_e_max: number;
}

export type Values = Geometry | Stage | Dark | Flat | Colour;
export type Kind = Values["kind"];

/**
 * A set's image, in blocks no more than 256 across or down: each block's
 * mean, by rows, its channels together, in linear units.
 */
export interface SetImage {
  width: number;
  height: number;
  channels: number;
  /** The pixels a block spans. */
  block: number;
  values: number[];
}

/** The image of the dark frame or flat field `id`. */
export function setImage(
  id: string,
  options: Limits & { on?: Server | undefined } = {},
): Promise<SetImage> {
  const address = (options.on ?? server).address(
    `/api/v1/calibration/image/${encodeURIComponent(id)}`,
  );
  return fetched(address, (answer) => answer.json() as Promise<SetImage>, options);
}
