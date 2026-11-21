// The histo scan's flow (W5.3), which its pane and the scan's view share:
// the step it is at, the regions proposed for the overview's tissue, kept,
// dropped, edited or drawn again, and the focus map's grid. Going back
// keeps each; a new overview proposes its own regions.
import { computed, ref, shallowRef } from "vue";

import type { Rect } from "@/ui/tile-viewer/coordinates";
import { rectShape, type Shape } from "@/ui/tile-viewer/shapes";

import type { OverviewView } from "../overview";
import { proposedOf, type Proposed, type Step } from "./histo";

/** An overview, told from another. */
const overviewKey = (view: OverviewView) =>
  [
    view.origin.x,
    view.origin.y,
    view.pixel_um,
    view.width,
    view.height,
    view.frames,
    view.area_mm2,
  ].join();

function flow() {
  const step = ref<Step>("overview");
  const regions = ref<Proposed[]>([]);
  /** The overview the regions were proposed for. */
  const proposedFor = shallowRef<string>();
  /** Which region a drag on the view draws again; past the last, a new one. */
  const redrawing = ref<number>();
  const columns = ref(3);
  const rows = ref(3);
  const surface = ref<"plane" | "thin_plate">("plane");

  /** Proposes `view`'s islands, unless they were for it already, or `again`. */
  function propose(view: OverviewView, again = false): void {
    const key = overviewKey(view);
    if (!again && key === proposedFor.value) return;
    proposedFor.value = key;
    regions.value = proposedOf(view);
    redrawing.value = undefined;
  }

  /** The region being drawn again, drawn: kept. */
  function drew(rect: Rect): void {
    const at = redrawing.value;
    if (at === undefined) return;
    const one = { rect, kept: true };
    regions.value =
      at < regions.value.length
        ? regions.value.map((was, i) => (i === at ? one : was))
        : [...regions.value, one];
    redrawing.value = undefined;
  }

  /** The regions on the view: those kept, and those dropped, fainter. */
  const shapes = computed<Shape[]>(() =>
    regions.value.map((one) => rectShape(one.kept ? "region" : "dropped", one.rect)),
  );

  return { step, regions, redrawing, columns, rows, surface, propose, drew, shapes };
}

export type HistoFlow = ReturnType<typeof flow>;

let shared: HistoFlow | undefined;
/** The page's histo scan, made once. */
export function useHisto(): HistoFlow {
  return (shared ??= flow());
}
