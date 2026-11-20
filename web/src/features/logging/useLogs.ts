// The records, kept live (W4.2): the media plane's tail sends the newest
// that match the filters, then each new one as it is made; an older page
// comes from `logs/records`, by the oldest kept. The tail's records are
// merged once a frame, however fast they come; a tail that closes is opened
// again, the filters asked anew.
import { onScopeDispose, shallowRef, watch, type Ref } from "vue";

import { logRecords, logTail, type LogRecord, type TailMessage } from "@/api/media";
import { merged, queryOf, type Filters } from "@/features/logging/logs";

/** How many of the newest records the tail sends first, and an older page holds. */
export const PAGE = 500;
/** How long to wait before opening a closed tail again, in ms. */
const AGAIN = 2_000;

export function useLogs(filters: Ref<Filters>) {
  const records = shallowRef<LogRecord[]>([]);
  /** Whether the tail has sent the newest records that match, and now follows. */
  const caughtUp = shallowRef(false);
  /** Records the tail passed over, as the page was behind. */
  const skipped = shallowRef(0);
  /** Whether older records may be there. */
  const older = shallowRef(true);
  const failed = shallowRef<string>();

  // The tail's records, merged once a frame.
  let pending: LogRecord[] = [];
  let flushing = false;
  function flush(): void {
    flushing = false;
    records.value = merged(records.value, pending);
    pending = [];
  }
  function told(message: TailMessage): void {
    if ("query" in message) {
      records.value = [];
      pending = [];
      caughtUp.value = false;
      skipped.value = 0;
      older.value = true;
      failed.value = undefined;
    } else if ("record" in message) {
      pending.push(message.record);
      if (!flushing) {
        flushing = true;
        requestAnimationFrame(flush);
      }
    } else if ("caught_up" in message) {
      caughtUp.value = true;
      // The newest that match, fewer than asked for: there is nothing older.
      older.value = records.value.length + pending.length >= PAGE;
    } else if ("skipped" in message) skipped.value += message.skipped;
    else if ("error" in message) failed.value = message.error;
  }

  let stop = new AbortController();
  let ask: ((query: ReturnType<typeof queryOf>) => void) | undefined;
  let leaving = false;
  function open(): void {
    stop = new AbortController();
    const tail = logTail(told, { signal: stop.signal, query: queryOf(filters.value, PAGE) });
    ask = tail.ask;
    const again = (): void => {
      ask = undefined;
      if (!leaving) setTimeout(open, AGAIN);
    };
    tail.ended.then(again, (error: unknown) => {
      failed.value = (error as Error).message;
      again();
    });
  }
  // The filters changed: asked again, or when the tail opens again.
  watch(
    filters,
    () => {
      try {
        ask?.(queryOf(filters.value, PAGE));
      } catch {
        // Not open yet: it asks with the filters as it opens.
      }
    },
    { deep: true },
  );
  onScopeDispose(() => {
    leaving = true;
    stop.abort();
  });
  open();

  /** The page of records older than the oldest kept. */
  async function showOlder(): Promise<void> {
    const first = records.value[0];
    if (!first) return;
    try {
      const page = await logRecords({ ...queryOf(filters.value), before: first.seq, limit: PAGE });
      records.value = merged(records.value, page.records);
      older.value = page.next !== undefined;
    } catch (error) {
      failed.value = (error as Error).message;
    }
  }

  return { records, caughtUp, skipped, older, failed, showOlder };
}
