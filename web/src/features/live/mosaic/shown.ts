// Whether the live mosaic is shown beside the live view (W3.8): kept between
// visits, for the live view's toolbar and the control tab's layout alike.
import type { Ref } from "vue";

import { kept } from "@/host/kept";

const isBoolean = (value: unknown): value is boolean => typeof value === "boolean";
let shown: Ref<boolean> | undefined;

/** Whether the mosaic is shown: one for the page, made when first asked for. */
export function useMosaicShown(): Ref<boolean> {
  return (shown ??= kept("oscope.control.mosaic", false, isBoolean));
}
