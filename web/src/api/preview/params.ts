// What a client asks the preview's stream for (ADR-0014, ADR-0035). Each
// message gives all of it and replaces the last: what it leaves out takes
// the `preview` Thing's defaults.
import type { Line, PixelRect } from "./message.ts";

/** The statistics asked for. */
export interface StatsRequest {
  /** The region to measure, the focus value's too: the preview's, without. */
  roi?: PixelRect | undefined;
  /** A line to profile. */
  profile?: Line | undefined;
  /** The histograms' bins, from 1 to 4096: 256 without. */
  bins?: number | undefined;
  /** Whether to send the saturation mask: yes, without. */
  mask?: boolean | undefined;
}

/** What the page asks for: everything but the acknowledgement, which the stream adds. */
export interface PreviewAsk {
  /** The viewport's width, in device pixels. */
  width: number;
  /** Its height. */
  height: number;
  /** The sensor's pixels to show: the whole frame, without. */
  roi?: PixelRect | undefined;
  /** The JPEG quality, from 1 to 100. */
  quality?: number | undefined;
  /** The most frames a second. */
  max_fps?: number | undefined;
  /** The statistics to send with frames, at most ten times a second. */
  stats?: StatsRequest | undefined;
}

/** One message's parameters: what the page asks, and the next message it is ready for. */
export type PreviewParams = PreviewAsk & { ack: number };
