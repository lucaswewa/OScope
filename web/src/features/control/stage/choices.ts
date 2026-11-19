// The stage's choices (W3.3), shown in its pane and taken by the image: the
// jog's steps, x and y's and z's, and whether a drag on the image moves the
// stage. Each is kept between visits.
import { defineStore } from "pinia";

import type { Axis } from "@/features/control/stage/stage";
import { kept } from "@/host/kept";

/** The steps along x and y, in µm, or jogging for as long as it is held. */
export const XY_STEPS = ["1", "10", "100", "1000", "continuous"] as const;
/** The steps along z, finer, for focusing. */
export const Z_STEPS = ["0.5", "2", "10", "50", "continuous"] as const;

const oneOf =
  (choices: readonly string[]) =>
  (value: unknown): value is string =>
    typeof value === "string" && choices.includes(value);
const isFlag = (value: unknown): value is boolean => typeof value === "boolean";

export const useChoices = defineStore("stage-choices", () => {
  const xy = kept("oscope.stage.xy", "100", oneOf(XY_STEPS));
  const z = kept("oscope.stage.z", "2", oneOf(Z_STEPS));
  /** Whether a drag on the image pans the stage, rather than the view. */
  const drags = kept("oscope.stage.drags", false, isFlag);

  /** The step along `axis`, in µm: none while jogging is continuous. */
  function size(axis: Axis): number | undefined {
    const chosen = axis === "z" ? z.value : xy.value;
    return chosen === "continuous" ? undefined : Number(chosen);
  }

  return { xy, z, drags, size };
});
