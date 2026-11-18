import { describe, expect, it } from "vitest";
import { h } from "vue";

import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import { draw } from "@/ui/test-kit";

const options = [
  { value: "histo", label: "Histo scan" },
  { value: "grid", label: "Grid scan" },
];

// Its list, and its keys, are tried in a real browser by the end-to-end
// tests: the list needs a page that is laid out.
describe("a list's field, closed", () => {
  const field = (props: Record<string, unknown> = {}) =>
    draw(Select, { options, ...props }).wrapper.get("[role='combobox']");

  it("writes the chosen option's label, from the options it was given", async () => {
    const drawn = draw(Select, { options, modelValue: "grid" });
    const trigger = drawn.wrapper.get("[role='combobox']");
    expect(trigger.text()).toBe("Grid scan");
    await drawn.set({ modelValue: "histo" });
    expect(trigger.text()).toBe("Histo scan");
    expect(trigger.find(".empty").exists()).toBe(false);
  });

  it("says that nothing is chosen, in its own words or the page's", () => {
    const none = field();
    expect(none.text()).toBe("Choose…");
    expect(none.find(".empty").exists()).toBe(true);
    expect(field({ placeholder: "No workflow" }).text()).toBe("No workflow");
    // A value that is none of its options is nothing chosen.
    expect(field({ modelValue: "gone" }).text()).toBe("Choose…");
  });

  it("says what it is when read-only, switched off, in error, or named", () => {
    expect(field({ readonly: true }).attributes("aria-readonly")).toBe("true");
    expect(field({ disabled: true }).attributes("disabled")).toBeDefined();
    expect(field({ invalid: true }).attributes("aria-invalid")).toBe("true");
    expect(field({ label: "Workflow" }).attributes("aria-label")).toBe("Workflow");
    expect(field({ compact: true }).classes()).toContain("compact");
  });

  it("takes what a page puts on it, though the library's root draws nothing", () => {
    const mine = field({ class: "mine", "data-kind": "workflow" });
    expect(mine.classes()).toEqual(expect.arrayContaining(["mine", "ui-field"]));
    expect(mine.attributes("data-kind")).toBe("workflow");
  });

  it("is named and described by the field around it", () => {
    const { wrapper } = draw(
      Field,
      { label: "Workflow", hint: "How the slide is scanned." },
      { default: () => h(Select, { options, modelValue: "grid" }) },
    );
    const trigger = wrapper.get("[role='combobox']");
    expect(wrapper.get("label").attributes("for")).toBe(trigger.attributes("id"));
    expect(trigger.attributes("aria-describedby")).toBe(wrapper.get("p").attributes("id"));
  });
});
