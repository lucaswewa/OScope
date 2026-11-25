// The project chosen or made (W3.7): a project made is listed and chosen,
// though the list asked for as the pane opened answers after it (W10.3).
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { describe, expect, it, vi } from "vitest";

import type { ProjectSummary } from "@/api/projects";

const pages: ((projects: ProjectSummary[]) => void)[] = [];
vi.mock("@/api/projects", () => ({
  projectPage: vi.fn(
    () =>
      new Promise((done) => pages.push((projects) => done({ projects, total: projects.length }))),
  ),
}));
vi.mock("@/api/things/use", () => ({
  useAction: () => ({ run: vi.fn(async () => ({ id: "new" })), running: { value: false } }),
}));

const one = (id: string, name: string) => ({ id, name }) as unknown as ProjectSummary;

describe("the project chosen or made", () => {
  it("chooses a project made, though the first list answers after the second", async () => {
    const { useProjectChoice } = await import("@/features/projects/projectChoice");
    let choice!: ReturnType<typeof useProjectChoice>;
    // The pane opened: its first list asked for, and still out.
    const pane = mount(
      defineComponent({
        setup() {
          choice = useProjectChoice("test.project");
          return () => h("div");
        },
      }),
    );
    await vi.waitFor(() => expect(pages).toHaveLength(1));
    choice.naming.value = "New";
    const making = choice.makeProject();
    await vi.waitFor(() => expect(pages).toHaveLength(2));
    // The second answers first, with the project made; the first after, without it.
    pages[1]!([one("old", "Old"), one("new", "New")]);
    await making;
    pages[0]!([one("old", "Old")]);
    await vi.waitFor(() => expect(choice.project.value).toBe("new"));
    await new Promise((settled) => setTimeout(settled));
    expect(choice.chosen.value?.name).toBe("New");
    pane.unmount();
  });
});
