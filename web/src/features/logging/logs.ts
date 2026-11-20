// The logging tab's sums (W4.2): the filters as `logs/records`' and the
// tail's query, the records kept, in order and each once, the newest 100,000
// at most, each level's count, and whether the list is at its end.
import type { Level, LogQuery, LogRecord } from "@/api/media";

/** The levels, the least serious first. */
export const LEVELS: readonly Level[] = ["trace", "debug", "info", "warn", "error", "critical"];

/** Each level's tone, from the tokens' states. */
export const TONES = {
  trace: "neutral",
  debug: "neutral",
  info: "info",
  warn: "warning",
  error: "danger",
  critical: "danger",
} as const satisfies Record<Level, "neutral" | "info" | "warning" | "danger">;

/** The most records kept: older ones go as the tail brings more. */
export const MOST = 100_000;

/** What the filters say: each may be empty. */
export interface Filters {
  level: Level;
  /** The source: a target's start, such as `oscope_api::stage`. */
  source: string;
  text: string;
  /** From and until, as a `datetime-local` field gives them: the page's own time. */
  since: string;
  until: string;
}

/** The query for `filters`: only what they say, the times in UTC. */
export function queryOf(filters: Filters, limit?: number): LogQuery {
  const query: LogQuery = { level: filters.level };
  if (filters.source.trim()) query.target = filters.source.trim();
  if (filters.text.trim()) query.text = filters.text.trim();
  const utc = (local: string) => {
    const time = new Date(local);
    return Number.isNaN(time.getTime()) ? undefined : time.toISOString();
  };
  const since = filters.since && utc(filters.since);
  const until = filters.until && utc(filters.until);
  if (since) query.since = since;
  if (until) query.until = until;
  if (limit !== undefined) query.limit = limit;
  return query;
}

/**
 * The records kept, with `more` among them: in order of their numbers, each
 * once, the newest `most`. The tail's records come after those kept, and an
 * older page's before them: neither needs a sort.
 */
export function merged(
  kept: readonly LogRecord[],
  more: readonly LogRecord[],
  most = MOST,
): LogRecord[] {
  if (more.length === 0) return kept as LogRecord[];
  const ordered = [...more].sort((a, b) => a.seq - b.seq);
  const first = kept[0]?.seq ?? Infinity;
  const last = kept.at(-1)?.seq ?? -Infinity;
  let all: LogRecord[];
  if (ordered[0]!.seq > last) all = [...kept, ...ordered];
  else if (ordered.at(-1)!.seq < first) all = [...ordered, ...kept];
  else {
    const bySeq = new Map(kept.map((record) => [record.seq, record]));
    for (const record of ordered) bySeq.set(record.seq, record);
    all = [...bySeq.values()].sort((a, b) => a.seq - b.seq);
  }
  return all.length > most ? all.slice(all.length - most) : all;
}

/** How many of each level the records hold. */
export function countsOf(records: readonly LogRecord[]): Record<Level, number> {
  const counts = Object.fromEntries(LEVELS.map((level) => [level, 0])) as Record<Level, number>;
  for (const record of records) counts[record.level] += 1;
  return counts;
}

/** Whether a list scrolled to `top` shows its end, to within `slack` pixels. */
export function atEnd(top: number, height: number, total: number, slack = 4): boolean {
  return top + height >= total - slack;
}

/** A size in bytes, said in the unit that keeps it short: 512 B, 12.3 kB, 4.5 MB. */
export function sizeOf(bytes: number): { value: string; unit: "B" | "kB" | "MB" | "GB" } {
  const units = ["B", "kB", "MB", "GB"] as const;
  let at = 0;
  let value = bytes;
  while (value >= 1000 && at < units.length - 1) {
    value /= 1000;
    at += 1;
  }
  return { value: at === 0 ? String(value) : value.toFixed(1), unit: units[at]! };
}
