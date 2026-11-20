// The overlays' choices (W3.5): which layers show, kept between visits;
// what the pointer draws with; and the ROI and the line drawn, in the
// sensor's pixels.
import { defineStore } from "pinia";
import { shallowRef } from "vue";

import type { Line, PixelRect } from "@/api/preview/message";
import { MAPS, type MapName } from "@/features/live/colourMaps";
import { kept } from "@/host/kept";

/** The layers that are only shown: switched on and off. */
export interface Layers {
  crosshair: boolean;
  grid: boolean;
  scale: boolean;
  /** The pixels at the sensor's limit, marked (W3.6). */
  saturated: boolean;
  /** The histogram, the look and the focus, under the image (W3.6). */
  values: boolean;
}

/** What a drag on the image draws: nothing, an ROI, or a line to profile. */
export type Tool = "none" | "roi" | "line";

const isLayers = (value: unknown): value is Layers =>
  typeof value === "object" &&
  value !== null &&
  ["crosshair", "grid", "scale", "saturated", "values"].every(
    (name) => typeof (value as Record<string, unknown>)[name] === "boolean",
  );

export const useOverlays = defineStore("overlays", () => {
  const layers = kept<Layers>(
    "oscope.live.layers",
    { crosshair: false, grid: false, scale: true, saturated: false, values: false },
    isLayers,
  );
  /** The colour map: the image's own colours, unless one is chosen (W3.6). */
  const map = kept<MapName>("oscope.live.map", "none", (value): value is MapName =>
    MAPS.includes(value as MapName),
  );
  const tool = shallowRef<Tool>("none");
  const roi = shallowRef<PixelRect>();
  const line = shallowRef<Line>();
  return { layers, map, tool, roi, line };
});
