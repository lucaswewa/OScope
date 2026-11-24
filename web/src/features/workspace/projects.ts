// The Workspace's lists of projects (W10.1): the Workspace keeps its views,
// each reading the projects as it is made, so each reads them again as its
// list opens, and a project made since is offered.
import type { ShallowRef } from "vue";

import { projectPage, type ProjectSummary } from "@/api/projects";

/** Reads the projects into `list` again; a failed read leaves it as it was. */
export async function relistProjects(list: ShallowRef<ProjectSummary[]>): Promise<void> {
  const page = await projectPage({ limit: 500 }).catch(() => undefined);
  if (page) list.value = page.projects;
}
