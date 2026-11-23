import { describe, expect, it } from "vitest";

import Button from "@/ui/Button.vue";
import IconButton from "@/ui/IconButton.vue";
import { draw, drawModel } from "@/ui/test-kit";
import ToggleButton from "@/ui/ToggleButton.vue";
import ToggleGroup from "@/ui/ToggleGroup.vue";

describe("a button", () => {
  it("is an outlined button unless told otherwise, and never submits a form by accident", () => {
    const plain = draw(Button, {}, { default: () => "Set home" }).wrapper.get("button");
    expect(plain.text()).toBe("Set home");
    expect(plain.attributes("type")).toBe("button");
    expect(plain.classes()).toEqual(expect.arrayContaining(["outline", "accent"]));
    expect(plain.classes()).not.toContain("compact");

    const props = { variant: "primary", tone: "danger", compact: true, type: "submit" };
    const loud = draw(Button, props, { default: () => "Delete all" }).wrapper.get("button");
    expect(loud.classes()).toEqual(expect.arrayContaining(["primary", "danger", "compact"]));
    expect(loud.attributes("type")).toBe("submit");
  });

  it("has its icon before its text, hidden from a screen reader", () => {
    const button = draw(Button, { icon: "camera" }, { default: () => "Capture" }).wrapper;
    const svg = button.get("button > svg");
    expect(svg.attributes("aria-hidden")).toBe("true");
    expect(button.get("button").element.firstElementChild).toBe(svg.element);
  });

  it("says it is busy, keeps its place, and takes no press", async () => {
    let presses = 0;
    const drawn = draw(
      Button,
      { icon: "camera", onClick: () => presses++ },
      { default: () => "Go" },
    );
    const button = drawn.wrapper.get("button");
    await button.trigger("click");
    expect(presses).toBe(1);

    await drawn.set({ busy: true });
    expect(button.attributes("aria-busy")).toBe("true");
    expect(button.attributes("aria-disabled")).toBe("true");
    // Still a button the keyboard reaches: the focus isn't thrown elsewhere.
    expect(button.attributes("disabled")).toBeUndefined();
    expect(button.find("svg").exists()).toBe(false);
    expect(button.text()).toContain("Working…");
    await button.trigger("click");
    expect(presses).toBe(1);

    await drawn.set({ busy: false });
    expect(button.attributes("aria-busy")).toBeUndefined();
    await button.trigger("click");
    expect(presses).toBe(2);
  });

  it("takes no press while switched off", async () => {
    let presses = 0;
    const off = draw(Button, { disabled: true, onClick: () => presses++ }, { default: () => "Go" });
    const button = off.wrapper.get("button");
    expect(button.attributes("disabled")).toBeDefined();
    (button.element as HTMLButtonElement).click();
    expect(presses).toBe(0);
  });
});

describe("an icon button", () => {
  it("has its label as its name, and an icon and nothing else to see", () => {
    const button = draw(IconButton, { icon: "refresh", label: "Refresh" }).wrapper.get("button");
    expect(button.attributes("aria-label")).toBe("Refresh");
    expect(button.text()).toBe("");
    expect(button.findAll("svg")).toHaveLength(1);
    expect(button.classes()).toEqual(expect.arrayContaining(["bare", "icon-button"]));
  });

  it("passes a press, and whatever a page puts on it, to its button", async () => {
    let presses = 0;
    const page = { onClick: () => presses++, class: "mine", "data-kind": "close" };
    const drawn = draw(IconButton, { icon: "close", label: "Close", ...page });
    const button = drawn.wrapper.get("button");
    expect(button.classes()).toEqual(expect.arrayContaining(["mine", "icon-button"]));
    expect(button.attributes("data-kind")).toBe("close");
    await button.trigger("click");
    expect(presses).toBe(1);

    await drawn.set({ disabled: true });
    (button.element as HTMLButtonElement).click();
    expect(presses).toBe(1);
  });

  it("is busy as a button is, with no icon under the ring", async () => {
    const drawn = draw(IconButton, { icon: "refresh", label: "Refresh", busy: true });
    const button = drawn.wrapper.get("button");
    expect(button.attributes("aria-busy")).toBe("true");
    expect(button.find("svg").exists()).toBe(false);
    expect(button.attributes("aria-label")).toBe("Refresh");
  });
});

describe("a button that stays pressed", () => {
  it("is pressed and let go by a press, and says which it is", async () => {
    const grid = drawModel(ToggleButton, false, {}, { default: () => "Grid" });
    const button = grid.wrapper.get("button");
    expect(button.attributes("aria-pressed")).toBe("false");
    await button.trigger("click");
    expect(button.attributes("aria-pressed")).toBe("true");
    expect(button.attributes("data-state")).toBe("on");
    await button.trigger("click");
    expect(grid.changes).toEqual([true, false]);
  });

  it("stays as it is while read-only, and takes no press while switched off", async () => {
    const fixed = drawModel(ToggleButton, true, { readonly: true }, { default: () => "Grid" });
    await fixed.wrapper.get("button").trigger("click");
    expect(fixed.changes).toEqual([]);
    expect(fixed.wrapper.get("button").attributes("aria-pressed")).toBe("true");

    const off = drawModel(ToggleButton, false, { disabled: true }, { default: () => "Grid" });
    (off.wrapper.get("button").element as HTMLButtonElement).click();
    expect(off.changes).toEqual([]);
  });
});

describe("a group of buttons that stay pressed", () => {
  const options = ["4", "10", "20", "40"].map((value) => ({ value, label: `${value}×` }));
  const pressed = (wrapper: ReturnType<typeof draw>["wrapper"]): string[] =>
    wrapper.findAll("button[data-state='on']").map((button) => button.text());

  it("has one pressed, which a press on another moves", async () => {
    const objective = drawModel<string | string[]>(ToggleGroup, "20", {
      options,
      label: "Objective",
    });
    expect(objective.wrapper.get("[role='group']").attributes("aria-label")).toBe("Objective");
    expect(pressed(objective.wrapper)).toEqual(["20×"]);
    await objective.wrapper.findAll("button")[3]!.trigger("click");
    expect(objective.changes).toEqual(["40"]);
    expect(pressed(objective.wrapper)).toEqual(["40×"]);
  });

  it("lets the pressed one go, unless one is required", async () => {
    const free = drawModel<string | string[]>(ToggleGroup, "20", { options });
    await free.wrapper.findAll("button")[2]!.trigger("click");
    expect(free.changes).toEqual([""]);
    expect(pressed(free.wrapper)).toEqual([]);

    const kept = drawModel<string | string[]>(ToggleGroup, "20", { options, required: true });
    await kept.wrapper.findAll("button")[2]!.trigger("click");
    expect(kept.changes).toEqual([]);
    expect(pressed(kept.wrapper)).toEqual(["20×"]);
  });

  it("has any number pressed with `multiple`, its value a list", async () => {
    const overlays = drawModel<string | string[]>(ToggleGroup, ["4"], { options, multiple: true });
    await overlays.wrapper.findAll("button")[2]!.trigger("click");
    expect(overlays.changes).toEqual([["4", "20"]]);
    await overlays.wrapper.findAll("button")[0]!.trigger("click");
    await overlays.wrapper.findAll("button")[2]!.trigger("click");
    expect(overlays.changes.at(-1)).toEqual([]);
    expect(pressed(overlays.wrapper)).toEqual([]);
  });

  it("stays as it is while read-only, and passes over a choice that is switched off", async () => {
    const fixed = drawModel<string | string[]>(ToggleGroup, "20", { options, readonly: true });
    await fixed.wrapper.findAll("button")[0]!.trigger("click");
    expect(fixed.changes).toEqual([]);

    const some = [...options.slice(0, 3), { value: "40", label: "40×", disabled: true }];
    const group = drawModel<string | string[]>(ToggleGroup, "20", { options: some });
    const last = group.wrapper.findAll("button")[3]!;
    expect(last.attributes("disabled")).toBeDefined();
    (last.element as HTMLButtonElement).click();
    expect(group.changes).toEqual([]);
  });
});
