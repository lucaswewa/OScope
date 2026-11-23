// The jobs tab's sums (W4.1): a job's progress and when it should end, the
// queue in its order and a queued job's new place, the history's pages, and
// where a job's results lead.
import type { Job, JobState } from "@/api/jobs";

/** The states of a job still in the queue. */
export const WAITING: ReadonlySet<JobState> = new Set(["queued", "running", "paused"]);

/** How far a job has got, from 0 to 100, if it says. */
export function percentOf(job: Job): number | undefined {
  const fraction = job.progress.fraction;
  if (fraction === null || fraction === undefined) return undefined;
  return Math.round(Math.min(1, Math.max(0, fraction)) * 100);
}

/** The server's estimate of the time left, as minutes and seconds, if it has one. */
export function timeLeft(
  seconds: number | null | undefined,
): { minutes: number; seconds: number } | undefined {
  if (seconds === null || seconds === undefined || !(seconds >= 0)) return undefined;
  const whole = Math.round(seconds);
  return { minutes: Math.floor(whole / 60), seconds: whole % 60 };
}

/** How long it ran, or has run so far, in seconds, if it started. */
export function ranFor(job: Job, now: number): number | undefined {
  if (!job.started) return undefined;
  const end = job.finished ? Date.parse(job.finished) : now;
  return Math.max(0, (end - Date.parse(job.started)) / 1000);
}

/** The queue in its order: what runs, then what is paused, then what waits, by its place. */
export function inOrder(queue: readonly Job[]): Job[] {
  const rank = (job: Job) => (job.state === "running" ? 0 : job.state === "paused" ? 1 : 2);
  return [...queue].sort(
    (a, b) => rank(a) - rank(b) || (a.position ?? Infinity) - (b.position ?? Infinity),
  );
}

/**
 * Where a waiting job goes for a key: Alt with the up or down arrow, one
 * place earlier or later; Alt with Home or End, the first or the last.
 * Nothing for any other key, or a move past the ends.
 */
export function movedTo(
  key: { key: string; altKey: boolean },
  position: number,
  waiting: number,
): number | undefined {
  if (!key.altKey) return undefined;
  const to = { ArrowUp: position - 1, ArrowDown: position + 1, Home: 0, End: waiting - 1 }[key.key];
  return to === undefined || to < 0 || to >= waiting || to === position ? undefined : to;
}

/** How many of the newest jobs the history asks for, a page at a time: 1,000 at most. */
export function historyLimit(pages: number, size: number): number {
  return Math.min(1000, pages * size);
}

/** Whether older jobs may be there: the answer was as long as asked, and more may be asked. */
export function mayHaveOlder(answered: number, limit: number): boolean {
  return answered >= limit && limit < 1000;
}

/** Where a result leads: an acquisition or a result of a project, in the gallery. */
export function resultPlace(path: string): { project: string; record: string } | undefined {
  const found = /^projects\/([^/]+)\/(acquisitions|results)\/([^/]+)$/.exec(path);
  return found ? { project: found[1]!, record: found[3]! } : undefined;
}
