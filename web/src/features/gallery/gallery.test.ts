// The Gallery's projects (W6.1): the grid's keys, the paging, the tags as
// typed, and what a deletion takes with it.
import { describe, expect, it } from "vitest";

import type { ProjectSummary } from "@/api/projects";

import {
  columnsOf,
  descriptionOf,
  nextCard,
  offsetOf,
  pageAt,
  pagesOf,
  tagsOf,
  whatGoes,
} from "./gallery";

describe("the grid's keys", () => {
  // Ten cards in rows of four: 0 1 2 3 / 4 5 6 7 / 8 9.
  const go = (key: string, from: number) => nextCard(key, from, 10, 4);

  it("go by one along a row, and by a row up and down", () => {
    expect(go("ArrowRight", 5)).toBe(6);
    expect(go("ArrowLeft", 5)).toBe(4);
    expect(go("ArrowDown", 5)).toBe(9);
    expect(go("ArrowUp", 5)).toBe(1);
    // Along, from a row's end to the next's start.
    expect(go("ArrowRight", 3)).toBe(4);
  });

  it("go to the first and the last", () => {
    expect(go("Home", 6)).toBe(0);
    expect(go("End", 2)).toBe(9);
  });

  it("go nowhere past an edge, or for another key", () => {
    expect(go("ArrowLeft", 0)).toBeUndefined();
    expect(go("ArrowUp", 2)).toBeUndefined();
    expect(go("ArrowDown", 6)).toBeUndefined();
    expect(go("ArrowRight", 9)).toBeUndefined();
    expect(go("Home", 0)).toBeUndefined();
    expect(go("Enter", 4)).toBeUndefined();
  });

  it("count a row's cards by how high they are", () => {
    expect(columnsOf([10, 10, 10.4, 10, 220, 220])).toBe(4);
    expect(columnsOf([10])).toBe(1);
    expect(columnsOf([])).toBe(1);
  });
});

describe("the paging", () => {
  it("counts the pages, one at least", () => {
    expect(pagesOf(0)).toBe(1);
    expect(pagesOf(24)).toBe(1);
    expect(pagesOf(25)).toBe(2);
    expect(pagesOf(49, 24)).toBe(3);
  });

  it("finds a page's start, and the page an offset is on", () => {
    expect(offsetOf(1)).toBe(0);
    expect(offsetOf(3)).toBe(48);
    expect(offsetOf(0)).toBe(0);
    expect(pageAt(0)).toBe(1);
    expect(pageAt(47)).toBe(2);
    expect(pageAt(48)).toBe(3);
  });
});

describe("a project's", () => {
  const project: ProjectSummary = {
    id: "p",
    name: "Tonsil",
    sample: "",
    tags: [],
    owner: "",
    created: "",
    modified: "",
    metadata: { description: "Stained with H&E", slide: 7 },
    acquisitions: 3,
    results: 0,
    reports: 1,
  };

  it("tags are as typed, a comma between, each once", () => {
    expect(tagsOf(" he, human ,he,, ")).toEqual(["he", "human"]);
    expect(tagsOf("")).toEqual([]);
  });

  it("deletion takes the records it has, by kind", () => {
    expect(whatGoes(project)).toEqual([
      { kind: "acquisitions", count: 3 },
      { kind: "reports", count: 1 },
    ]);
  });

  it("description is its metadata's, if a text", () => {
    expect(descriptionOf(project)).toBe("Stained with H&E");
    expect(descriptionOf({ ...project, metadata: { description: 7 } })).toBe("");
  });
});
