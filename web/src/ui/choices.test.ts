import { afterEach, describe, expect, it } from "vitest";

import Checkbox from "@/ui/Checkbox.vue";
import RadioGroup from "@/ui/RadioGroup.vue";
import Switch from "@/ui/Switch.vue";
import { draw, drawModel } from "@/ui/test-kit";

afterEach(() => {
  document.body.innerHTML = "";
});

const label = { default: () => "Invert x" };

describe("a checkbox", () => {
  it("is ticked and unticked by a press, and its label is around it", async () => {
    const invert = drawModel(Checkbox, false, {}, label);
    const box = invert.wrapper.get("[role='checkbox']");
    expect(box.attributes("aria-checked")).toBe("false");
    // The label holds the box, so a press on its words is a press on the box:
    // the end-to-end test presses them in a real browser.
    expect(invert.wrapper.get("label").text()).toBe("Invert x");
    expect(invert.wrapper.get("label").element.contains(box.element)).toBe(true);

    await box.trigger("click");
    expect(box.attributes("aria-checked")).toBe("true");
    await box.trigger("click");
    expect(invert.changes).toEqual([true, false]);
  });

  it("is neither ticked nor not while some of what it stands for is", async () => {
    const some = drawModel(Checkbox, false, { indeterminate: true }, label);
    const box = some.wrapper.get("[role='checkbox']");
    expect(box.attributes("aria-checked")).toBe("mixed");
    await box.trigger("click");
    expect(some.changes).toEqual([true]);
  });

  it("shows what is so while read-only, and changes by no press", async () => {
    const fixed = drawModel(Checkbox, true, { readonly: true }, label);
    const box = fixed.wrapper.get("[role='checkbox']");
    expect(box.attributes("aria-readonly")).toBe("true");
    await box.trigger("click");
    expect(fixed.changes).toEqual([]);
    expect(box.attributes("aria-checked")).toBe("true");
    // The keyboard still reaches it.
    expect(box.attributes("disabled")).toBeUndefined();
  });

  it("takes no press while switched off", () => {
    const off = drawModel(Checkbox, false, { disabled: true }, label);
    const box = off.wrapper.get("[role='checkbox']");
    expect(box.attributes("disabled")).toBeDefined();
    (box.element as HTMLButtonElement).click();
    expect(off.changes).toEqual([]);
  });
});

describe("a switch", () => {
  it("is turned on and off by a press, and says which it is", async () => {
    const live = drawModel(Switch, false, {}, { default: () => "Live image" });
    const track = live.wrapper.get("[role='switch']");
    expect(track.attributes("aria-checked")).toBe("false");
    await track.trigger("click");
    expect(track.attributes("aria-checked")).toBe("true");
    await track.trigger("click");
    expect(live.changes).toEqual([true, false]);
  });

  it("stays as it is while read-only, and takes no press while switched off", async () => {
    const fixed = drawModel(Switch, true, { readonly: true }, label);
    const track = fixed.wrapper.get("[role='switch']");
    expect(track.attributes("aria-readonly")).toBe("true");
    await track.trigger("click");
    expect(fixed.changes).toEqual([]);
    expect(track.attributes("aria-checked")).toBe("true");

    const off = drawModel(Switch, false, { disabled: true }, label);
    (off.wrapper.get("[role='switch']").element as HTMLButtonElement).click();
    expect(off.changes).toEqual([]);
  });

  it("takes its name from `label` where no text beside it gives one", () => {
    const bare = draw(Switch, { label: "Live image" }).wrapper.get("[role='switch']");
    expect(bare.attributes("aria-label")).toBe("Live image");
  });
});

describe("a group of radio buttons", () => {
  const options = [
    { value: "snake", label: "Snake" },
    { value: "raster", label: "Raster" },
    { value: "spiral", label: "Spiral", disabled: true },
  ];
  const chosen = (wrapper: ReturnType<typeof draw>["wrapper"]): string[] =>
    wrapper
      .findAll("[role='radio'][aria-checked='true']")
      .map((radio) => radio.attributes("value")!);

  it("has one chosen, which a press on another moves", async () => {
    const order = drawModel<string | undefined>(RadioGroup, "snake", {
      options,
      label: "Scan order",
    });
    const group = order.wrapper.get("[role='radiogroup']");
    expect(group.attributes("aria-label")).toBe("Scan order");
    expect(group.attributes("aria-orientation")).toBe("vertical");
    expect(order.wrapper.findAll("label").map((one) => one.text())).toEqual([
      "Snake",
      "Raster",
      "Spiral",
    ]);
    expect(chosen(order.wrapper)).toEqual(["snake"]);

    await order.wrapper.findAll("[role='radio']")[1]!.trigger("click");
    expect(order.changes).toEqual(["raster"]);
    expect(chosen(order.wrapper)).toEqual(["raster"]);
  });

  it("starts with none chosen where nothing is given, and lies in a row where asked", async () => {
    const order = drawModel<string | undefined>(RadioGroup, undefined, { options, row: true });
    expect(chosen(order.wrapper)).toEqual([]);
    expect(order.wrapper.get("[role='radiogroup']").attributes("aria-orientation")).toBe(
      "horizontal",
    );
  });

  it("passes over a choice that is switched off, and stays as it is while read-only", async () => {
    const order = drawModel<string | undefined>(RadioGroup, "snake", { options });
    const spiral = order.wrapper.findAll("[role='radio']")[2]!;
    expect(spiral.attributes("disabled")).toBeDefined();
    (spiral.element as HTMLButtonElement).click();
    expect(order.changes).toEqual([]);

    const fixed = drawModel<string | undefined>(RadioGroup, "snake", { options, readonly: true });
    expect(fixed.wrapper.get("[role='radiogroup']").attributes("aria-readonly")).toBe("true");
    await fixed.wrapper.findAll("[role='radio']")[1]!.trigger("click");
    expect(fixed.changes).toEqual([]);
    expect(chosen(fixed.wrapper)).toEqual(["snake"]);
  });
});
