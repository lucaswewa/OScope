// The overlays' choices (W3.5): which layers show, kept between visits;
// what the pointer draws with; and the ROI and the line drawn, in the
// sensor's pixels.
import { defineStore } from "pinia";
import { shallowRef } from "vue";

import type { Line, PixelRect } from "@/api/preview/message";
import { kept } from "@/host/kept";

/** The layers that are only shown: switched on and off. */
export interface Layers {
  crosshair: boolean;
  grid: boolean;
  scale: boolean;
}

/** What a drag on the image draws: nothing, an ROI, or a line to profile. */
export type Tool = "none" | "roi" | "line";

const isLayers = (value: unknown): value is Layers =>
  typeof value === "object" &&
  value !== null &&
  ["crosshair", "grid", "scale"].every(
    (name) => typeof (value as Record<string, unknown>)[name] === "boolean",
  );

export const useOverlays = defineStore("overlays", () => {
  const layers = kept<Layers>(
    "oscope.live.layers",
    { crosshair: false, grid: false, scale: true },
    isLayers,
  );
  const tool = shallowRef<Tool>("none");
  const roi = shallowRef<PixelRect>();
  const line = shallowRef<Line>();
  return { layers, tool, roi, line };
});
