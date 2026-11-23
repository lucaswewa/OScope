// The Workspace's analyses (P48, W7.4): a recipe run on a project's image,
// its results, one made again, two compared, and one deleted. The actions
// go through the generated types; the endpoints, which the OpenAPI document
// doesn't describe, are typed here from the store's records, and checked
// against a real server by the contract tests.
import { act, type Following } from "./actions.ts";
import { fetched, server, type Limits, type Schema, type Server } from "./client.ts";
import type { JobRef } from "./jobs.ts";
import { fileAddress } from "./media.ts";

type On = Limits & { on?: Server | undefined };

/** An analysis: a recipe, the image it runs on, and a region of it in µm. */
export type Analysis = Schema<"workspace_run_input">["analysis"];
/** A region of an image, in µm from its top-left corner. */
export type Region = NonNullable<Analysis["roi"]>;

/** How a result was made (P48): enough to make it again. */
export interface Provenance {
  recipe: unknown;
  nodes: { node: string; version: string; extension?: string | null }[];
  inputs: { kind: "acquisition" | "result"; id: string }[];
  hashes?: Record<string, string>;
  software: string;
}

/** A result, as the Workspace lists it: its record, and the files in its folder. */
export interface Analysed {
  id: string;
  project: string;
  name: string;
  created: string;
  provenance: Provenance;
  image?: unknown;
  files: string[];
}

/** What differs between two results' provenance: where, as a JSON Pointer, and the two values. */
export interface Change {
  path: string;
  before: unknown;
  after: unknown;
}

/** What a result's `summary.json` says it read and made: each table's rows among it. */
export interface Summary {
  recipe: string;
  input: string;
  pixels: [number, number, number, number];
  roi_um: Region | null;
  outputs: Record<string, string>;
  rows: Record<string, number>;
}

const read = <T>(path: string, options: On) =>
  fetched((options.on ?? server).address(path), (answer) => answer.json() as Promise<T>, options);
const id = encodeURIComponent;

/** A project's results, the newest last. */
export function resultList(project: string, options: On = {}): Promise<Analysed[]> {
  return read(`/api/v1/workspace/results/${id(project)}`, options);
}

/** What differs between results `a` and `b` of `project`, in how they were made. */
export function difference(project: string, a: string, b: string, options: On = {}) {
  return read<Change[]>(`/api/v1/workspace/difference/${id(project)}/${id(a)}/${id(b)}`, options);
}

/** What result `result` of `project` read and made. */
export function summaryOf(project: string, result: string, options: On = {}): Promise<Summary> {
  const address = fileAddress(project, `results/${result}/summary.json`, options.on ?? server);
  return fetched(address, (answer) => answer.json() as Promise<Summary>, options);
}

/** Runs `analysis` on an image of `project`: a `workspace.run` job. */
export function analyse(project: string, analysis: Analysis, options: Following = {}) {
  return act("/api/v1/workspace/run", { project, analysis }, options) as Promise<JobRef>;
}

/**
 * Makes result `result` of `project` again, its nodes' parameters changed
 * as `changes` says, by node ID: a job.
 */
export function rerun(
  project: string,
  result: string,
  changes: Record<string, Record<string, unknown>> = {},
  options: Following = {},
): Promise<JobRef> {
  return act("/api/v1/workspace/rerun", { project, result, changes }, options) as Promise<JobRef>;
}

/** Deletes result `result` of `project`, with its files. */
export async function deleteResult(
  project: string,
  result: string,
  options: Following = {},
): Promise<void> {
  await act("/api/v1/workspace/delete_result", { project, result }, options);
}
