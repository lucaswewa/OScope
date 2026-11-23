// Reports (P47, W7.9): made by the job `report.make` through a template the
// projects Thing keeps; and a template read, previewed, saved and deleted.
// The actions go through the generated types; the endpoints, which the
// OpenAPI document doesn't describe, are typed here from P47's `Template`.
import { act, type Following } from "./actions.ts";
import { fetched, server, type Limits, type Server } from "./client.ts";
import type { JobRef } from "./jobs.ts";

type On = Limits & { on?: Server | undefined };

/** A report template: its Typst, for PDF, and its minijinja, for HTML. */
export interface Template {
  name: string;
  typst: string;
  html: string;
  /** OScope's own, which can't be changed. */
  builtin: boolean;
}

/** A report's forms. */
export type Format = "pdf" | "html";

/** What a report is made of: a project's results, through a template, in its forms. */
export interface Making {
  results: string[];
  template: string;
  formats: Format[];
}

const read = <T>(path: string, options: On) =>
  fetched((options.on ?? server).address(path), (answer) => answer.json() as Promise<T>, options);

/** The templates' names, OScope's own first. */
export function templateNames(options: On = {}): Promise<string[]> {
  return read("/api/v1/projects/templates", options);
}

/** A template, by name. */
export function templateNamed(name: string, options: On = {}): Promise<Template> {
  return read(`/api/v1/projects/template/${encodeURIComponent(name)}`, options);
}

/** A template kept, once a sample report renders through it: refused, saying where, if not. */
export async function saveTemplate(
  template: Pick<Template, "name" | "typst" | "html">,
  options: Following = {},
): Promise<void> {
  const { name, typst, html } = template;
  await act("/api/v1/projects/save_template", { name, typst, html }, options);
}

/** A user's template deleted. */
export async function deleteTemplate(name: string, options: Following = {}): Promise<void> {
  await act("/api/v1/projects/delete_template", { name }, options);
}

/**
 * A template's sources previewed, kept nowhere: on a project's result, or
 * on the sample report without one. The PDF's bytes, and the HTML.
 */
export async function previewTemplate(
  sources: Pick<Template, "typst" | "html">,
  on?: { project: string; result: string },
  options: Following = {},
): Promise<{ pdf: Uint8Array<ArrayBuffer>; html: string }> {
  const input = { typst: sources.typst, html: sources.html, ...on };
  const made = await act("/api/v1/projects/preview_template", input, options);
  return { pdf: Uint8Array.from(atob(made.pdf), (c) => c.charCodeAt(0)), html: made.html };
}

/** A report on `project`, made as `making` says: the job. */
export function makeReport(
  project: string,
  making: Making,
  options: Following = {},
): Promise<JobRef> {
  const { results, template, formats } = making;
  const params = { project, options: { params: { results }, template, formats } };
  return act("/api/v1/jobs/submit", { kind: "report.make", params }, options) as Promise<JobRef>;
}
