// The scan tab's planning (W5.1), which its panes and its view share: the
// pattern and what it needs, the region drawn, and the server's plan of it,
// asked for again a moment after each change. Nothing moves.
import { computed, ref, shallowRef, watch } from "vue";

import { act } from "@/api/actions";
import { ApiError } from "@/api/errors";
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

import { requestOf, type Preview, type Stack } from "./plan";

/** How long after a change the plan is asked for again, in ms. */
const SETTLE_MS = 300;

/** The tab's planning: one for the page, its panes and its view alike. */
function planning() {
  const pattern = ref("snake");
  /** The share of a frame its neighbours overlap. */
  const overlap = ref(0.1);
  const region = shallowRef<Rect>();
  const stack = ref<Stack>({ below_um: 10, above_um: 10, step_um: 2 });
  const at = shallowRef<Point>();
  const positions = ref<Point[]>([]);
  const params = ref<unknown>();
  /** Whether a drag on the view draws the region. */
  const drawing = ref(false);

  const request = computed(() =>
    requestOf({
      pattern: pattern.value,
      overlap: overlap.value,
      region: region.value,
      stack: stack.value,
      at: at.value,
      positions: positions.value,
      params: params.value,
    }),
  );
  const preview = shallowRef<Preview>();
  const refused = shallowRef<string>();
  const asking = ref(false);

  let timer: ReturnType<typeof setTimeout> | undefined;
  let current: AbortController | undefined;
  /** Asks for the plan of the request now, the last asked winning. */
  async function ask(): Promise<void> {
    current?.abort();
    const asked = request.value;
    if (!asked) {
      preview.value = undefined;
      refused.value = undefined;
      return;
    }
    const mine = (current = new AbortController());
    asking.value = true;
    try {
      const planned = await act("/api/v1/scan/plan", { request: asked }, { signal: mine.signal });
      if (mine.signal.aborted) return;
      preview.value = planned;
      refused.value = undefined;
    } catch (thrown) {
      if (mine.signal.aborted) return;
      preview.value = undefined;
      refused.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    } finally {
      if (current === mine) asking.value = false;
    }
  }
  watch(
    request,
    () => {
      clearTimeout(timer);
      timer = setTimeout(() => void ask(), SETTLE_MS);
    },
    { deep: true },
  );

  return {
    pattern,
    overlap,
    region,
    stack,
    at,
    positions,
    params,
    drawing,
    request,
    preview,
    refused,
    asking,
    ask,
  };
}

export type Planning = ReturnType<typeof planning>;

let shared: Planning | undefined;
/** The page's planning, made once. */
export function usePlanning(): Planning {
  return (shared ??= planning());
}
