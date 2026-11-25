// A project chosen, or made, for what is about to be taken (W3.7, W5.2):
// the projects listed, the one chosen kept between visits under `key`, and
// a new one made by its name. The capture pane and the scan's run share it.
import { computed, onMounted, shallowRef } from "vue";

import { ApiError } from "@/api/errors";
import { projectPage, type ProjectSummary } from "@/api/projects";
import { useAction } from "@/api/things/use";
import { kept } from "@/host/kept";

const isText = (value: unknown): value is string => typeof value === "string";

/** The projects, and the one chosen, kept under `key`. */
export function useProjectChoice(key: string) {
  const make = useAction("/api/v1/projects/create");
  const project = kept(key, "", isText);
  const projects = shallowRef<ProjectSummary[]>([]);
  /** Why the projects couldn't be listed, if they couldn't. */
  const unlisted = shallowRef<string>();

  /**
   * Lists the projects, the last asked for winning: the list asked for as
   * the pane opens can answer after the one asked for once a project is
   * made, and would hide it (W10.3).
   */
  let asked = 0;
  async function list(): Promise<void> {
    const mine = ++asked;
    try {
      const page = await projectPage({ limit: 500 });
      if (mine !== asked) return;
      projects.value = page.projects;
      unlisted.value = undefined;
    } catch (thrown) {
      if (mine !== asked) return;
      unlisted.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    }
  }
  onMounted(list);

  const choices = computed(() => projects.value.map((one) => ({ value: one.id, label: one.name })));
  /** The project chosen, if it is listed. */
  const chosen = computed(() => projects.value.find((one) => one.id === project.value));

  /** A new project's name, as typed. */
  const naming = shallowRef("");
  /** Makes the project named, and chooses it. */
  async function makeProject(): Promise<void> {
    const name = naming.value.trim();
    if (!name) return;
    const made = await make.run({ name });
    if (!made) return;
    naming.value = "";
    await list();
    project.value = made.id;
  }

  return {
    project,
    projects,
    unlisted,
    choices,
    chosen,
    naming,
    making: make.running,
    makeProject,
    list,
  };
}

export type ProjectChoice = ReturnType<typeof useProjectChoice>;
