// A scan's log of 100,000 lines, for the gallery's table: made up, and the
// same at every visit, so that a test can say what it shows.

export type Level = "debug" | "info" | "warning" | "error";

export interface LogLine {
  n: number;
  time: string;
  level: Level;
  source: string;
  message: string;
}

const LEVELS: readonly Level[] = [
  "info",
  "info",
  "debug",
  "info",
  "warning",
  "info",
  "info",
  "error",
];
const SOURCES = ["camera", "stage", "scan", "stitch", "autofocus", "jobs", "server"];
const MESSAGES = [
  "Frame captured",
  "Moved to the next tile",
  "Focus found",
  "Tile stitched",
  "Exposure set",
  "Job queued",
  "Job done",
  "Request answered",
  "Background checked",
];

/** When the scan began: 18 November 2026, 09:00 UTC. */
const BEGAN = Date.UTC(2026, 10, 18, 9, 0, 0);

/** `count` lines, 137 ms apart, each level, source and message in turn. */
export function logLines(count: number): readonly LogLine[] {
  return Object.freeze(
    Array.from({ length: count }, (_, at) => ({
      n: at + 1,
      time: new Date(BEGAN + at * 137).toISOString().slice(11, 23),
      level: LEVELS[at % LEVELS.length]!,
      source: SOURCES[(at * 3) % SOURCES.length]!,
      message: `${MESSAGES[(at * 5) % MESSAGES.length]!} (${at + 1})`,
    })),
  );
}
