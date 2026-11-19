// Streaming (W3.1): the camera started and stopped, and how the live image
// stands: not streaming, waiting for its first frame, live, or stale when
// its frames have stopped coming while the camera streams.
import { computed, onScopeDispose, shallowRef, watch, type Ref } from "vue";

import { useAction, useProperty } from "@/api/things/use";

/** How the live image stands. */
export type LiveState = "off" | "waiting" | "live" | "stale";

/** The least time without a frame, while streaming, before the image is stale. */
export const STALE_AFTER = 2_000;

/** What the state is decided from, on the page's clock, in ms. */
export interface Arrivals {
  /** Whether the camera streams, as last read. */
  streaming: boolean | undefined;
  /** When this page saw it start streaming. */
  since: number;
  /** When the last frame came, and the gap before it. */
  last: number | undefined;
  gap: number | undefined;
  now: number;
}

/**
 * The live image's state. Stale is after three of its frames' gaps without
 * one, and never sooner than `STALE_AFTER`: a long exposure is slow, not stale.
 */
export function liveState({ streaming, since, last, gap, now }: Arrivals): LiveState {
  if (!streaming) return "off";
  if (last === undefined || last < since) return "waiting";
  return now - last > Math.max(STALE_AFTER, 3 * (gap ?? 0)) ? "stale" : "live";
}

/**
 * The camera's streaming, and the live image's state from the frames drawn,
 * counted by `drawn`: for the viewer, for as long as it lasts.
 */
export function useStreaming(drawn: Ref<number>) {
  const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
  const start = useAction("/api/v1/camera/start_streaming");
  const stop = useAction("/api/v1/camera/stop_streaming");
  const now = shallowRef(performance.now());
  const since = shallowRef(0);
  const last = shallowRef<number>();
  const gap = shallowRef<number>();
  watch(
    () => streaming.value.value === true,
    (on) => {
      if (on) since.value = performance.now();
    },
    { immediate: true },
  );
  watch(drawn, () => {
    const at = performance.now();
    if (last.value !== undefined) gap.value = at - last.value;
    last.value = at;
  });
  const ticking = setInterval(() => (now.value = performance.now()), 500);
  onScopeDispose(() => clearInterval(ticking));
  return {
    state: computed(() =>
      liveState({
        streaming: streaming.value.value as boolean | undefined,
        since: since.value,
        last: last.value,
        gap: gap.value,
        now: now.value,
      }),
    ),
    busy: computed(() => start.running.value || stop.running.value),
    start: () => void start.run({}),
    stop: () => void stop.run({}),
  };
}
