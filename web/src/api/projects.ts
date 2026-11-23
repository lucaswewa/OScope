// Projects (P28): a page of them, as `projects/list` gives it. That route's
// answer isn't in the OpenAPI document: its shape is written here, and
// checked against a real server by the contract tests. And what goes in
// and comes out (W6.3): the formats, uploads, and the recordings waiting.
import { fetched, server, type Limits, type Server } from "./client.ts";
import { ApiError } from "./errors.ts";
import { send, type Progress } from "./transfer.ts";

/** A project, and how much it holds. */
export interface ProjectSummary {
  id: string;
  name: string;
  /** What is on the slide. */
  sample: string;
  tags: string[];
  owner: string;
  created: string;
  modified: string;
  metadata: Record<string, unknown>;
  acquisitions: number;
  results: number;
  reports: number;
}

/** A page of projects, the most recently changed first. */
export interface ProjectPage {
  /** How many the query finds in all. */
  total: number;
  /** How many were skipped before this page. */
  offset: number;
  projects: ProjectSummary[];
}

/**
 * Which projects a page lists: those whose name, sample or tags hold
 * `text`, or with the whole `tag`; the most recently changed first, or by
 * name (W6.1); `limit` of them (50, and 500 at most) from `offset`.
 */
export interface ProjectQuery {
  text?: string;
  tag?: string;
  sort?: "changed" | "name";
  offset?: number;
  limit?: number;
}

/** A page of the projects, as `query` asks. */
export function projectPage(
  query: ProjectQuery = {},
  options: Limits & { on?: Server } = {},
): Promise<ProjectPage> {
  const search = new URLSearchParams(
    Object.entries(query)
      .filter(([, value]) => value !== undefined && value !== "")
      .map(([key, value]) => [key, String(value)]),
  ).toString();
  const path = `/api/v1/projects/list${search ? `?${search}` : ""}`;
  return fetched((options.on ?? server).address(path), (answer) => answer.json(), options);
}

/**
 * A project's thumbnail, its newest image's, or one of its records' (W6.2),
 * a JPEG: none while it has no image.
 */
export async function projectThumbnail(
  id: string,
  options: Limits & { on?: Server; record?: string } = {},
): Promise<Blob | undefined> {
  const record = options.record ? `/${encodeURIComponent(options.record)}` : "";
  const address = (options.on ?? server).address(
    `/api/v1/projects/thumbnail/${encodeURIComponent(id)}${record}`,
  );
  try {
    return await fetched(address, (answer) => answer.blob(), options);
  } catch (thrown) {
    if (thrown instanceof ApiError && thrown.status === 404) return undefined;
    throw thrown;
  }
}

/**
 * A project, with its acquisitions, results and reports (W6.2): as the
 * store's records are, which the Gallery's project page reads.
 */
export function projectContents(
  id: string,
  options: Limits & { on?: Server } = {},
): Promise<unknown> {
  const address = (options.on ?? server).address(
    `/api/v1/projects/project/${encodeURIComponent(id)}`,
  );
  return fetched(address, (answer) => answer.json() as Promise<unknown>, options);
}

/** A file format the server imports or exports (ADR-0037). */
export interface Format {
  id: string;
  name: string;
  /** Its files' extensions, without the dot: `tif`, `ome.tif`. */
  extensions: string[];
}

/** What the server imports, and exports to: OScope's own, then its extensions'. */
export interface Formats {
  importers: Format[];
  exporters: Format[];
}

/** The formats the server imports and exports (W6.3). */
export function projectFormats(options: Limits & { on?: Server } = {}): Promise<Formats> {
  const address = (options.on ?? server).address("/api/v1/projects/formats");
  return fetched(address, (answer) => answer.json() as Promise<Formats>, options);
}

/** A file the upload route staged (ADR-0051), which an import names. */
export interface Staged {
  id: string;
  name: string;
  bytes: number;
  /** When it goes, if nothing has used it. */
  expires: string;
}

/** Uploads `file`, told as it goes, to be staged for an import (W6.3). */
export async function upload(
  file: File,
  options: Limits & { on?: Server; progress?: (now: Progress) => void } = {},
): Promise<Staged> {
  const address = (options.on ?? server).address(`/api/v1/upload/${encodeURIComponent(file.name)}`);
  return (await send(address, file, options)) as Staged;
}

/** A recording waiting in the scratch space (ADR-0022), to be ingested. */
export interface Recording {
  id: string;
  name: string;
  created: string;
  /** When it ended: none while it records. */
  ended?: string | null;
  /** Why it ended early, if it did. */
  error?: string | null;
  /** Whether a crash cut it short, and it was recovered. */
  recovered: boolean;
  frames: number;
  bytes: number;
  /** Frames the camera dropped between those it holds. */
  dropped: number;
}

/** The recordings waiting in the scratch space, the newest first (W6.3). */
export function waitingRecordings(options: Limits & { on?: Server } = {}): Promise<Recording[]> {
  const address = (options.on ?? server).address("/api/v1/system/recordings");
  return fetched(address, (answer) => answer.json() as Promise<Recording[]>, options);
}
