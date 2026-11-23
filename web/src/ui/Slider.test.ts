import { describe, expect, it } from "vitest";

import Slider from "@/ui/Slider.vue";
import { drawModel } from "@/ui/test-kit";

/** A slider holding `value`, its thumb, and where it said it was let go. */
function slider(value: number, props: Record<string, unknown> = {}) {
  const commits: number[] = [];
  const drawn = drawModel(Slider, value, {
    ...props,
    onCommit: (now: number) => commits.push(now),
  });
  const thumb = drawn.wrapper.get("[role='slider']");
  return { ...drawn, commits, thumb, key: (key: string) => thumb.trigger("keydown", { key }) };
}

describe("a slider", () => {
  // The thumb's keys are tried in a real browser, by the end-to-end tests:
  // the library finds its thumb only once the page is laid out.
  it("says its name, range and unit, and has its number beside it", async () => {
    const exposure = slider(20, { min: 1, max: 200, unit: "ms", label: "Exposure" });
    await exposure.wrapper.vm.$nextTick();
    expect(exposure.thumb.attributes()).toMatchObject({
      "aria-label": "Exposure",
      "aria-valuemin": "1",
      "aria-valuemax": "200",
      "aria-valuetext": "20 ms",
    });
    const number = exposure.wrapper.get("[role='spinbutton']");
    expect((number.element as HTMLInputElement).value).toBe("20");
    expect(number.attributes("aria-label")).toBe("Exposure");
    expect(slider(5, { bare: true }).wrapper.find("[role='spinbutton']").exists()).toBe(false);
  });

  it("is moved by the number beside it, which is a value let go", async () => {
    const exposure = slider(20, { min: 1, max: 200, unit: "ms" });
    const number = exposure.wrapper.get("[role='spinbutton']");
    (number.element as HTMLInputElement).focus();
    await number.setValue("50");
    await number.trigger("keydown", { key: "Enter" });
    expect(exposure.changes).toEqual([50]);
    expect(exposure.commits).toEqual([50]);
    expect(exposure.thumb.attributes("aria-valuetext")).toBe("50 ms");

    // What the number refuses doesn't move the thumb.
    await number.setValue("500");
    await number.trigger("keydown", { key: "Enter" });
    expect(exposure.changes).toEqual([50]);
    expect(exposure.wrapper.get(".reason").text()).toBe("At most 200.");
  });

  it("stays where it is while read-only or switched off", async () => {
    const fixed = slider(60, { readonly: true });
    expect(fixed.thumb.attributes("aria-readonly")).toBe("true");
    await fixed.key("ArrowRight");
    await fixed.key("Home");
    expect(fixed.changes).toEqual([]);
    expect(fixed.held.value).toBe(60);

    const off = slider(60, { disabled: true });
    await off.key("ArrowRight");
    expect(off.changes).toEqual([]);
    expect(off.wrapper.get("[role='spinbutton']").attributes("disabled")).toBeDefined();
  });
});
