// Projects (P28): a page of them, as `projects/list` gives it. That route's
// answer isn't in the OpenAPI document: its shape is written here, and
// checked against a real server by the contract tests.
import { fetched, server, type Limits, type Server } from "./client.ts";
import { ApiError } from "./errors.ts";

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

/** A project's thumbnail, its newest image's, a JPEG: none while it has no image. */
export async function projectThumbnail(
  id: string,
  options: Limits & { on?: Server } = {},
): Promise<Blob | undefined> {
  const address = (options.on ?? server).address(
    `/api/v1/projects/thumbnail/${encodeURIComponent(id)}`,
  );
  try {
    return await fetched(address, (answer) => answer.blob(), options);
  } catch (thrown) {
    if (thrown instanceof ApiError && thrown.status === 404) return undefined;
    throw thrown;
  }
}
