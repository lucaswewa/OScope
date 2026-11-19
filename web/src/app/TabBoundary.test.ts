// A tab's own failures (W2.1): an error inside the tab stops it, says why,
// and offers to load it again; a page that couldn't load does the same; and
// another tab starts afresh.
import { mount } from "@vue/test-utils";
import { defineComponent, h, nextTick } from "vue";
import { describe, expect, it } from "vitest";

import { i18n } from "@/app/i18n";
import TabBoundary from "@/app/TabBoundary.vue";

let breaks = true;
let drawn = 0;
const Fragile = defineComponent({
  setup() {
    drawn++;
    return () => {
      if (breaks) throw new Error("the stage's pad broke");
      return h("p", { class: "tab-page" }, "the tab");
    };
  },
});

function mounted() {
  return mount(TabBoundary, {
    props: { tab: "control" },
    slots: { default: () => h(Fragile) },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
}

describe("a tab's boundary", () => {
  it("stops a tab that fails, says why, and loads it again", async () => {
    [breaks, drawn] = [true, 0];
    const wrapper = mounted();
    await nextTick();
    const failed = wrapper.get("[data-said='failed']");
    expect(failed.attributes("role")).toBe("alert");
    expect(failed.text()).toContain("It failed: the stage's pad broke");
    breaks = false;
    await wrapper.get("button").trigger("click");
    expect(wrapper.find("[data-said='failed']").exists()).toBe(false);
    expect([wrapper.get(".tab-page").text(), drawn]).toEqual(["the tab", 2]);
    wrapper.unmount();
  });

  it("tries again a page that couldn't load, and clears its failure for another tab", async () => {
    [breaks, drawn] = [false, 0];
    const wrapper = mounted();
    const boundary = wrapper.vm as unknown as { fail(error: unknown, retry?: () => void): void };
    let tried = 0;
    boundary.fail(new Error("a chunk is missing"), () => tried++);
    await nextTick();
    expect(wrapper.get("[data-said='failed']").text()).toContain("a chunk is missing");
    await wrapper.get("button").trigger("click");
    expect(tried).toBe(1);
    boundary.fail(new Error("a chunk is missing"));
    await nextTick();
    await wrapper.setProps({ tab: "jobs" });
    expect(wrapper.find("[data-said='failed']").exists()).toBe(false);
    expect(wrapper.get(".tab-page").text()).toBe("the tab");
    wrapper.unmount();
  });
});
