// The jobs, kept live (W4.1): the queue from `jobs/queue`, which the jobs
// Thing's events read again, and the history from `jobs/list`, a page more
// at a time, fetched again as jobs change, at most once a second.
import { computed, onScopeDispose, shallowRef, watch } from "vue";

import { ApiError } from "@/api/errors";
import { job as jobNow, jobList, type Job } from "@/api/jobs";
import { useEvent, useProperty } from "@/api/things/use";
import { historyLimit, inOrder, mayHaveOlder, WAITING } from "@/features/jobs/jobs";

/** A page of the history. */
export const PAGE = 25;

export function useJobs() {
  const queued = useProperty("/api/v1/jobs/queue", { every: 2_000 });
  const changed = useEvent<Job>("/api/v1/jobs/changed", { keep: 1 });
  // The queue's answer has its own schema, the same fields as `Job`'s.
  const queue = computed(() => inOrder((queued.value.value ?? []) as unknown as Job[]));

  const pages = shallowRef(1);
  const history = shallowRef<Job[]>([]);
  const older = shallowRef(false);
  const failed = shallowRef<string>();
  let last = 0;
  async function fetchHistory(): Promise<void> {
    last = Date.now();
    const limit = historyLimit(pages.value, PAGE);
    try {
      const jobs = await jobList({ limit });
      history.value = jobs.filter((one) => !WAITING.has(one.state));
      older.value = mayHaveOlder(jobs.length, limit);
      failed.value = undefined;
    } catch (thrown) {
      failed.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    }
  }
  /** Fetched again at most once a second, however many jobs change. */
  let timer: ReturnType<typeof setTimeout> | undefined;
  function soon(): void {
    if (timer) return;
    const wait = Math.max(0, 1_000 - (Date.now() - last));
    timer = setTimeout(() => {
      timer = undefined;
      void fetchHistory();
    }, wait);
  }
  watch(() => changed.last.value, soon);
  onScopeDispose(() => clearTimeout(timer));
  void fetchHistory();

  /** A page more of the history: older jobs. */
  function showOlder(): void {
    pages.value += 1;
    void fetchHistory();
  }

  /** A job asked for by its ID that neither list has, as the server has it now. */
  const fetched = shallowRef<Job>();
  async function fetchOne(id: string): Promise<void> {
    try {
      fetched.value = await jobNow(id);
    } catch {
      fetched.value = undefined;
    }
  }
  const listed = (id: string) =>
    queue.value.find((one) => one.id === id) ?? history.value.find((one) => one.id === id);
  /** A job by its ID: from the queue, which is live; the history; or as last fetched. */
  function find(id: string | undefined): Job | undefined {
    if (id === undefined) return undefined;
    return listed(id) ?? (fetched.value?.id === id ? fetched.value : undefined);
  }
  /** Fetches the job `id` from the server, when neither list has it. */
  function fetchMissing(id: string | undefined): void {
    if (id !== undefined && !listed(id) && fetched.value?.id !== id) void fetchOne(id);
  }

  return { queue, history, older, failed, showOlder, find, fetchMissing };
}
