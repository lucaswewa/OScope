// The status in full (W4.3): what the server measured of a subsystem, named,
// each measure in its unit.
import { describe, expect, it } from "vitest";

import { detailsOf } from "./details";

const WORDS: Record<string, string> = { available_gb: "Free", temperature_c: "Temperature" };
const word = (measure: string) => WORDS[measure];

describe("a subsystem's details", () => {
  it("are named, each measure in the unit its name ends with", () => {
    const disks = { available_gb: 171.84, filling_mb_s: 12.4, recordings: 2 };
    const gpu = { temperature_c: 61, memory_used_mib: 812 };
    expect([...detailsOf(disks, word), ...detailsOf(gpu, word)]).toEqual([
      { term: "Free", value: "171.8 GB" },
      { term: "filling mb s", value: "12 MB/s" },
      { term: "recordings", value: "2" },
      { term: "Temperature", value: "61 °C" },
      { term: "memory used mib", value: "812 MiB" },
    ]);
  });

  it("give a list and words as they are, and say what wasn't measured", () => {
    const told = {
      problems: ["one: no manifest", "two: no driver"],
      name: "a GPU",
      temperature_c: null,
    };
    expect(detailsOf(told, word)).toEqual([
      { term: "problems", value: "one: no manifest; two: no driver" },
      { term: "name", value: "a GPU" },
      { term: "Temperature", value: "—" },
    ]);
  });

  it("are none when the server measured nothing", () => {
    expect(detailsOf(null, word)).toEqual([]);
    expect(detailsOf([1, 2], word)).toEqual([]);
    expect(detailsOf("a word", word)).toEqual([]);
  });
});
