// An action that hands its work to a job (ADR-0016), run from the control
// pane (W3.2): started, its job followed to its end, and what became of it.
// A refusal of the action itself is the shell's to tell (W2.1); how the job
// ended is told where it was started.
import { onScopeDispose, shallowRef } from "vue";

import { finished, type Job, type JobRef } from "@/api/jobs";
import { ApiError } from "@/api/errors";
import type { ActionPath, Input } from "@/api/actions";
import { useAction } from "@/api/things/use";

/** The action at `path`, whose output is a job to follow. */
export function useJobAction<P extends ActionPath>(path: P) {
  const action = useAction(path);
  const job = shallowRef<Job>();
  const failed = shallowRef<string>();
  const running = shallowRef(false);
  const leaving = new AbortController();
  onScopeDispose(() => leaving.abort());

  /** Runs it, and follows its job: the job once completed, or undefined. */
  async function run(input: Input<P>): Promise<Job | undefined> {
    running.value = true;
    failed.value = undefined;
    job.value = undefined;
    try {
      const ref = (await action.run(input)) as JobRef | undefined;
      if (!ref) return undefined;
      return await finished(ref, { signal: leaving.signal, seen: (now) => (job.value = now) });
    } catch (thrown) {
      if (!leaving.signal.aborted) {
        failed.value = thrown instanceof ApiError ? thrown.message : String(thrown);
      }
      return undefined;
    } finally {
      running.value = false;
    }
  }

  return { run, job, failed, running };
}
