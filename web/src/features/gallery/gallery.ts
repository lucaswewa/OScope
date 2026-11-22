// The Gallery's projects (W6.1): their pages, the grid walked with the
// keys, the tags typed, and what a project's deletion takes with it.
import type { ProjectSummary } from "@/api/projects";

/** How many cards a page shows. */
export const PAGE_SIZE = 24;

/** The pages `total` projects take, one at least. */
export function pagesOf(total: number, size = PAGE_SIZE): number {
  return Math.max(1, Math.ceil(total / size));
}

/** The page, from 1, that holds the project at `offset`. */
export function pageAt(offset: number, size = PAGE_SIZE): number {
  return Math.floor(offset / size) + 1;
}

/** Where page `page`, from 1, starts. */
export function offsetOf(page: number, size = PAGE_SIZE): number {
  return (Math.max(1, page) - 1) * size;
}

/**
 * The card a key goes to, from the card at `index` of `count`, laid out in
 * rows of `columns`: the arrows by one, up and down by a row, Home and End
 * to the first and the last. Nothing for any other key, or past an edge.
 */
export function nextCard(
  key: string,
  index: number,
  count: number,
  columns: number,
): number | undefined {
  const across = Math.max(1, columns);
  const to = {
    ArrowLeft: index - 1,
    ArrowRight: index + 1,
    ArrowUp: index - across,
    ArrowDown: index + across,
    Home: 0,
    End: count - 1,
  }[key];
  return to === undefined || to < 0 || to >= count || to === index ? undefined : to;
}

/** How many cards a row holds: those as high on the page as the first. */
export function columnsOf(tops: readonly number[]): number {
  if (tops.length === 0) return 1;
  const first = tops[0]!;
  return Math.max(1, tops.filter((top) => Math.abs(top - first) < 1).length);
}

/** Tags as typed, a comma between: each trimmed, once, none empty. */
export function tagsOf(typed: string): string[] {
  return [...new Set(typed.split(",").map((tag) => tag.trim()))].filter(Boolean);
}

/** What deleting a project takes with it: its records, by kind, those it has. */
export function whatGoes(project: ProjectSummary): { kind: string; count: number }[] {
  return (["acquisitions", "results", "reports"] as const)
    .map((kind) => ({ kind, count: project[kind] }))
    .filter((one) => one.count > 0);
}

/** A project's description: its metadata's, if it has one. */
export function descriptionOf(project: ProjectSummary): string {
  const description = project.metadata.description;
  return typeof description === "string" ? description : "";
}
