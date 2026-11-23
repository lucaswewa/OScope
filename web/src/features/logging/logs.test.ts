import { describe, expect, it } from "vitest";

import type { LogRecord } from "@/api/media";
import { windowOf } from "@/ui/table";

import { atEnd, countsOf, merged, MOST, queryOf, sizeOf, type Filters } from "./logs";

const none: Filters = { level: "info", source: "", text: "", since: "", until: "" };

/** Records numbered `from` up to but not including `to`, as the server numbers them. */
function records(from: number, to: number): LogRecord[] {
  const levels = ["trace", "debug", "info", "warn", "error", "critical"] as const;
  return Array.from({ length: to - from }, (_, i) => ({
    seq: from + i,
    time: new Date(Date.UTC(2026, 10, 20, 10, 0, 0, from + i)).toISOString(),
    level: levels[(from + i) % levels.length]!,
    target: "oscope_api::stage",
    message: `record ${from + i}`,
  }));
}

describe("the filters' query", () => {
  it("says only what the filters say", () => {
    expect(queryOf(none)).toEqual({ level: "info" });
    const filters = {
      ...none,
      level: "warn" as const,
      source: " oscope_api::stage ",
      text: "limit",
    };
    expect(queryOf(filters, 200)).toEqual({
      level: "warn",
      target: "oscope_api::stage",
      text: "limit",
      limit: 200,
    });
  });

  it("gives the times in UTC, from the page's own, and leaves out what isn't a time", () => {
    const filters = { ...none, since: "2026-11-20T10:00", until: "not a time" };
    expect(queryOf(filters).since).toBe(new Date("2026-11-20T10:00").toISOString());
    expect(queryOf(filters)).not.toHaveProperty("until");
  });
});

describe("the records kept", () => {
  it("takes the tail's after them, and an older page's before them", () => {
    const kept = records(100, 110);
    expect(merged(kept, records(110, 113)).map((r) => r.seq)).toEqual([
      ...Array.from({ length: 13 }, (_, i) => 100 + i),
    ]);
    expect(merged(kept, records(95, 100))[0]!.seq).toBe(95);
  });

  it("keeps each record once, in order, whatever came twice or out of turn", () => {
    const kept = records(100, 110);
    const more = [...records(105, 112)].reverse();
    expect(merged(kept, more).map((r) => r.seq)).toEqual(
      Array.from({ length: 12 }, (_, i) => 100 + i),
    );
  });

  it("holds the newest 100,000, older ones going as the tail brings more", () => {
    const full = merged([], records(1, MOST + 1));
    expect(full).toHaveLength(MOST);
    const later = merged(full, records(MOST + 1, MOST + 1_001));
    expect(later).toHaveLength(MOST);
    expect([later[0]!.seq, later.at(-1)!.seq]).toEqual([1_001, MOST + 1_000]);
    // A page older than everything kept, the window full: nothing older is kept.
    expect(merged(later, records(1, 11))[0]!.seq).toBe(1_001);
  });

  it("shows a window of the 100,000, a screenful and a few more, wherever it is scrolled", () => {
    const rows = 24;
    expect(windowOf(0, 600, rows, MOST)).toEqual({ first: 0, last: 31 });
    expect(windowOf((MOST - 25) * rows, 600, rows, MOST)).toEqual({
      first: MOST - 30,
      last: MOST,
    });
  });

  it("counts each level", () => {
    expect(countsOf(records(0, 12))).toEqual({
      trace: 2,
      debug: 2,
      info: 2,
      warn: 2,
      error: 2,
      critical: 2,
    });
  });
});

describe("the tail's end", () => {
  it("is shown when the list is scrolled to its foot, give or take a few pixels", () => {
    expect(atEnd(400, 600, 1000)).toBe(true);
    expect(atEnd(397, 600, 1000)).toBe(true);
    expect(atEnd(390, 600, 1000)).toBe(false);
    expect(atEnd(0, 600, 500)).toBe(true);
  });
});

describe("a daily file's size", () => {
  it("is said in the unit that keeps it short", () => {
    expect([512, 12_345, 4_500_000, 2e9, 7e12].map(sizeOf)).toEqual([
      { value: "512", unit: "B" },
      { value: "12.3", unit: "kB" },
      { value: "4.5", unit: "MB" },
      { value: "2.0", unit: "GB" },
      { value: "7000.0", unit: "GB" },
    ]);
  });
});
