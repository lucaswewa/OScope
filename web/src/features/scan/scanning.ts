// The tab's scan (W5.2): the latest scan's progress, which the server keeps
// whoever started it, and its job, followed while it goes; and whether the
// view shows it, not the plan: while it goes, and after the run asked here,
// until the plan changes.
import { computed, onScopeDispose, shallowRef, watch } from "vue";

import { JOB_ENDED, job as jobNow, type Job } from "@/api/jobs";
import { useProperty } from "@/api/things/use";

import { usePlanning } from "./planning";
import { going } from "./running";

/** How often the scan is read, in ms. */
const EVERY_MS = 1_000;

/** The latest scan, followed for as long as the calling scope lasts. */
export function useScan() {
  const planning = usePlanning();
  const read = useProperty("/api/v1/scan/progress", { every: EVERY_MS });
  const progress = computed(() => read.value.value ?? undefined);
  const job = shallowRef<Job>();

  // Its job, read each second until it ends; a new scan's from the start.
  let timer: ReturnType<typeof setTimeout> | undefined;
  let following = 0;
  async function follow(mine: number, id: string): Promise<void> {
    try {
      const now = await jobNow(id);
      if (mine !== following) return;
      job.value = now;
      if (JOB_ENDED.has(now.state)) return;
    } catch {
      // Read again in a moment.
    }
    if (mine === following) timer = setTimeout(() => void follow(mine, id), EVERY_MS);
  }
  watch(
    () => progress.value?.job,
    (id) => {
      clearTimeout(timer);
      const mine = ++following;
      job.value = undefined;
      if (id) void follow(mine, id);
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    following++;
    clearTimeout(timer);
  });

  const goes = computed(() => (progress.value ? going(progress.value, job.value) : false));
  /** Whether the run asked here is still to be shown, though it has ended. */
  const ran = shallowRef(false);
  watch(planning.requestText, () => {
    if (!goes.value) ran.value = false;
  });
  const shown = computed(() => progress.value !== undefined && (goes.value || ran.value));

  return {
    progress,
    job,
    goes,
    shown,
    /** Shows the scan just started. */
    show: () => (ran.value = true),
  };
}

export type Scan = ReturnType<typeof useScan>;
