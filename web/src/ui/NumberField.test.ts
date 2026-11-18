import { afterEach, describe, expect, it } from "vitest";

import NumberField from "@/ui/NumberField.vue";
import { drawModel } from "@/ui/test-kit";

/** A field holding `value`, with what a user does to it. */
function field(value: number | null, props: Record<string, unknown> = {}) {
  const drawn = drawModel<number | null>(NumberField, value, props);
  const input = drawn.wrapper.get("input");
  return {
    ...drawn,
    input,
    element: input.element as HTMLInputElement,
    async type(text: string): Promise<void> {
      (input.element as HTMLInputElement).focus();
      await input.setValue(text);
    },
    key: (key: string) => input.trigger("keydown", { key }),
    wheel: (deltaY: number) => input.trigger("wheel", { deltaY }),
    reason: () => drawn.wrapper.find('[role="alert"]'),
  };
}

describe("a number field", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("takes what is typed on Enter, and on leaving, but not at each key", async () => {
    const x = field(1250, { min: -5000, max: 5000 });
    await x.type("12");
    await x.type("125");
    expect(x.changes).toEqual([]);
    await x.key("Enter");
    expect(x.changes).toEqual([125]);

    await x.type("7.5");
    await x.input.trigger("blur");
    expect(x.changes).toEqual([125, 7.5]);

    // The same value again says nothing.
    await x.type("7.50");
    await x.key("Enter");
    expect(x.changes).toEqual([125, 7.5]);
    expect(x.element.value).toBe("7.5");
  });

  it("refuses what isn't a number, says why, and keeps the value it had", async () => {
    const x = field(1250, { min: -5000, max: 5000 });
    expect(x.reason().exists()).toBe(false);
    expect(x.input.attributes("aria-invalid")).toBeUndefined();

    await x.type("12,5");
    await x.key("Enter");
    expect(x.changes).toEqual([]);
    expect(x.held.value).toBe(1250);
    expect(x.reason().text()).toBe("This isn't a number.");
    expect(x.input.attributes("aria-invalid")).toBe("true");
    expect(x.input.attributes("aria-describedby")).toBe(x.reason().attributes("id"));
    // What was typed stays, to be put right.
    expect(x.element.value).toBe("12,5");

    await x.type("12.5");
    await x.key("Enter");
    expect(x.changes).toEqual([12.5]);
    expect(x.reason().exists()).toBe(false);
    expect(x.input.attributes("aria-invalid")).toBeUndefined();
  });

  it("refuses a value outside its range, naming the end it passed", async () => {
    const overlap = field(0.35, { min: 0.1, max: 0.7 });
    await overlap.type("0.9");
    await overlap.input.trigger("blur");
    expect(overlap.reason().text()).toBe("At most 0.7.");
    await overlap.type("0");
    await overlap.key("Enter");
    expect(overlap.reason().text()).toBe("At least 0.1.");
    await overlap.type("");
    await overlap.key("Enter");
    expect(overlap.reason().text()).toBe("A number is needed.");
    expect(overlap.changes).toEqual([]);
  });

  it("goes back to its value on Escape", async () => {
    const x = field(1250);
    await x.type("nonsense");
    await x.key("Enter");
    expect(x.reason().exists()).toBe(true);
    await x.key("Escape");
    expect(x.element.value).toBe("1250");
    expect(x.reason().exists()).toBe(false);
    expect(x.changes).toEqual([]);
  });

  it("takes no value at all, where it is optional", async () => {
    const delay = field(5, { optional: true, min: 0 });
    await delay.type("");
    await delay.key("Enter");
    expect(delay.changes).toEqual([null]);
    expect(delay.reason().exists()).toBe(false);
    // A step from nothing starts at the range's lower end.
    await delay.key("ArrowUp");
    expect(delay.changes).toEqual([null, 1]);
  });

  it("moves by its step with the arrow keys, by ten with the page keys, each taken at once", async () => {
    const x = field(1250, { min: -5000, max: 5000, step: 10 });
    await x.key("ArrowUp");
    await x.key("ArrowUp");
    await x.key("ArrowDown");
    await x.key("PageUp");
    await x.key("PageDown");
    await x.key("PageDown");
    expect(x.changes).toEqual([1260, 1270, 1260, 1360, 1260, 1160]);
    expect(x.element.value).toBe("1160");
  });

  it("moves from what is typed, if that is a number", async () => {
    const x = field(1250, { step: 10 });
    await x.type("40");
    await x.key("ArrowUp");
    expect(x.changes).toEqual([50]);
    await x.type("forty");
    await x.key("ArrowUp");
    expect(x.changes).toEqual([50, 60]);
  });

  it("goes to its range's ends with Home and End, and stops there", async () => {
    const overlap = field(0.35, { min: 0.1, max: 0.7, step: 0.05, digits: 2 });
    await overlap.key("End");
    await overlap.key("ArrowUp");
    await overlap.key("Home");
    await overlap.key("ArrowDown");
    await overlap.key("ArrowUp");
    expect(overlap.changes).toEqual([0.7, 0.1, 0.15]);
    expect(overlap.element.value).toBe("0.15");

    // Without a range, Home and End are the text's own keys.
    const free = field(3);
    await free.key("Home");
    await free.key("End");
    expect(free.changes).toEqual([]);
  });

  it("is moved by the wheel only while it has the focus", async () => {
    const x = field(1250, { step: 10 });
    const away = await x.wheel(-100);
    expect(x.changes).toEqual([]);
    expect(away).toBeUndefined();

    x.element.focus();
    await x.wheel(-100);
    await x.wheel(-100);
    await x.wheel(100);
    expect(x.changes).toEqual([1260, 1270, 1260]);
  });

  it("stops the page scrolling under a wheel it uses, and not otherwise", () => {
    const x = field(1250);
    const turn = (): boolean => {
      const event = new WheelEvent("wheel", { deltaY: -100, cancelable: true, bubbles: true });
      x.element.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(turn()).toBe(false);
    x.element.focus();
    expect(turn()).toBe(true);
  });

  it("shows a value from outside at once, but not under the hand that is typing", async () => {
    const x = field(1250);
    await x.set({ modelValue: 1300 });
    expect(x.element.value).toBe("1300");

    await x.type("14");
    await x.set({ modelValue: 1310 });
    expect(x.element.value).toBe("14");
    await x.key("Escape");
    expect(x.element.value).toBe("1310");
  });

  it("changes by nothing while read-only or switched off", async () => {
    for (const state of [{ readonly: true }, { disabled: true }]) {
      const x = field(36.6, { ...state, min: 0, max: 100 });
      await x.key("ArrowUp");
      await x.key("End");
      x.element.focus();
      await x.wheel(-100);
      await x.input.trigger("blur");
      expect(x.changes, JSON.stringify(state)).toEqual([]);
      expect(x.element.value).toBe("36.6");
    }
  });

  it("says what it is to a screen reader: a spin button, its value, range and unit", async () => {
    const x = field(1250, { min: -5000, max: 5000, unit: "µm", label: "x" });
    expect(x.input.attributes()).toMatchObject({
      role: "spinbutton",
      inputmode: "decimal",
      "aria-label": "x",
      "aria-valuenow": "1250",
      "aria-valuemin": "-5000",
      "aria-valuemax": "5000",
      "aria-valuetext": "1250 µm",
    });
    // The unit is drawn, and said through the value's text, not twice.
    expect(x.wrapper.get(".unit").attributes("aria-hidden")).toBe("true");
    expect(x.wrapper.get(".unit").text()).toBe("µm");

    const empty = field(null, { optional: true, unit: "s" });
    expect(empty.input.attributes("aria-valuenow")).toBeUndefined();
    expect(empty.input.attributes("aria-valuetext")).toBeUndefined();
  });
});
