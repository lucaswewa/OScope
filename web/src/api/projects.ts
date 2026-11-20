// Projects (P28): a page of them, as `projects/list` gives it. That route's
// answer isn't in the OpenAPI document: its shape is written here, and
// checked against a real server by the contract tests.
import { fetched, server, type Limits, type Server } from "./client.ts";

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

/** A page of the projects: `limit` of them (50, and 500 at most) from `offset`. */
export function projectPage(
  query: { text?: string; offset?: number; limit?: number } = {},
  options: Limits & { on?: Server } = {},
): Promise<ProjectPage> {
  const search = new URLSearchParams(
    Object.entries(query).map(([key, value]) => [key, String(value)]),
  ).toString();
  const path = `/api/v1/projects/list${search ? `?${search}` : ""}`;
  return fetched((options.on ?? server).address(path), (answer) => answer.json(), options);
}
