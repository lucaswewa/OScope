import type { DOMWrapper } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { nextTick } from "vue";

import type { Fault } from "@/ui/schema-form/check";
import SchemaForm from "@/ui/schema-form/SchemaForm.vue";
import type { Json } from "@/ui/schema-form/shape";
import { draw, drawModel } from "@/ui/test-kit";

const scan = {
  type: "object",
  properties: {
    name: { type: "string", minLength: 1, description: "What the scan is called. People see it." },
    exposure_ms: { type: "number", minimum: 0.1, maximum: 1000, default: 20, unit: "ms" },
    tiles: { type: "integer", minimum: 1, maximum: 400, default: 16 },
    autofocus: { type: "boolean", default: true },
  },
  required: ["name"],
};

/** A form of `schema`, held as a page holds it, and what it says. */
function form(schema: unknown, value?: Json, props: Record<string, unknown> = {}) {
  const faults: Fault[][] = [];
  const drawn = drawModel(SchemaForm, value, {
    schema,
    label: "The scan",
    onFaults: (now: Fault[]) => faults.push(now),
    ...props,
  });
  const fields = () => drawn.wrapper.findAll(".field");
  /** The field with a label, or the one before it whose label is quiet. */
  const field = (label: string): DOMWrapper<Element> => {
    const found = fields().find((one) => one.find("label").text() === label);
    if (!found)
      throw new Error(
        `no field ${label}: ${fields()
          .map((one) => one.find("label").text())
          .join(", ")}`,
      );
    return found;
  };
  const button = (name: string) =>
    drawn.wrapper
      .findAll("button")
      .find((one) => (one.attributes("aria-label") ?? one.text()) === name);
  return {
    ...drawn,
    faults,
    field,
    button,
    /** The value the page holds now. */
    now: () => drawn.held.value,
    /** What a field says below it: a hint, or what is wrong; nothing, if neither. */
    note: (label: string): string => {
      const note = field(label).find(".note");
      return note.exists() ? note.text() : "";
    },
  };
}

const type = async (
  input: Pick<DOMWrapper<Element>, "setValue" | "trigger">,
  text: string,
  enter = false,
): Promise<void> => {
  await input.setValue(text);
  if (enter) await input.trigger("keydown", { key: "Enter" });
  await nextTick();
};

describe("a form drawn from a JSON Schema", () => {
  it("starts from the schema's defaults, and gives the value and its faults", () => {
    const drawn = form(scan);
    expect(drawn.changes[0]).toEqual({ name: "", exposure_ms: 20, tiles: 16, autofocus: true });
    expect(drawn.faults.at(-1)).toEqual([{ path: ["name"], why: "needed" }]);
    expect((drawn.field("Exposure (ms)").find("input").element as HTMLInputElement).value).toBe(
      "20",
    );
    expect(drawn.wrapper.get("[role=group]").attributes("aria-label")).toBe("The scan");
    // A field with a default is given, from its default: there is no box to leave it out.
    expect(drawn.wrapper.findAll("[role=checkbox]")).toHaveLength(0);
  });

  it("starts from the schema's defaults with no page holding its value too", () => {
    // Vue makes a value that isn't given `false` where it may be a boolean, as JSON may.
    const faults: Fault[][] = [];
    const { wrapper } = draw(SchemaForm, {
      schema: scan,
      label: "The scan",
      onFaults: (now: Fault[]) => faults.push(now),
    });
    expect(faults.at(-1)).toEqual([{ path: ["name"], why: "needed" }]);
    expect(wrapper.find("[role=spinbutton]").element).toHaveProperty("value", "20");
  });

  it("names each field in words, with the first sentence of what it is as its hint", () => {
    const drawn = form(scan);
    expect(drawn.wrapper.findAll(".field > label.label").map((one) => one.text())).toEqual([
      "Name",
      "Exposure (ms)",
      "Tiles",
      "Autofocus",
    ]);
    expect(drawn.note("Name")).toBe("Needed.");
    expect(drawn.note("Exposure (ms)")).toBe("");
    const described = form({
      type: "object",
      properties: { gain: { type: "number", description: "The gain. It is one at first." } },
    });
    expect(described.wrapper.text()).toContain("The gain.");
    expect(described.wrapper.text()).not.toContain("one at first");
  });

  it("changes the value as its fields are changed", async () => {
    const drawn = form(scan);
    await type(drawn.field("Name").get("input"), "slide 4");
    await type(drawn.field("Tiles").get("input"), "25", true);
    await drawn.field("Autofocus").get("button").trigger("click");
    expect(drawn.now()).toEqual({ name: "slide 4", exposure_ms: 20, tiles: 25, autofocus: false });
    expect(drawn.faults.at(-1)).toEqual([]);
  });

  it("says at a field why its value is wrong, and a number typed out of range isn't taken", async () => {
    const drawn = form(scan, { name: "slide 4", exposure_ms: 20, tiles: 0, autofocus: true });
    expect(drawn.note("Tiles")).toBe("At least 1.");
    await type(drawn.field("Exposure (ms)").get("input"), "5000", true);
    expect(drawn.field("Exposure (ms)").text()).toContain("At most 1000.");
    expect((drawn.now() as Record<string, Json>).exposure_ms).toBe(20);
  });

  it("gives a value that may be left out, or leaves it out", async () => {
    const drawn = form({
      type: "object",
      properties: {
        gains: {
          type: "object",
          properties: { red: { type: "number", default: 1 } },
          description: "The gains.",
        },
        note: { type: ["string", "null"] },
      },
      required: ["note"],
    });
    expect(drawn.now()).toEqual({ note: null });
    const [gains, note] = drawn.wrapper.findAll("[role=checkbox]");
    expect(drawn.wrapper.text()).toContain("The gains.");
    await gains!.trigger("click");
    expect(drawn.now()).toEqual({ note: null, gains: { red: 1 } });
    expect(drawn.wrapper.findAll("legend").map((one) => one.text())).toEqual(["Gains"]);
    await note!.trigger("click");
    expect(drawn.now()).toEqual({ note: "", gains: { red: 1 } });
    // Its label is said, under the box that shows it.
    expect(drawn.field("Note").get("label").classes()).toContain("quiet");
    await gains!.trigger("click");
    await note!.trigger("click");
    // A field that needn't be there goes; one that must be is null.
    expect(drawn.now()).toEqual({ note: null });
  });

  it("draws a union's kind, and what that kind holds", async () => {
    const schema = {
      type: "object",
      properties: {
        region: {
          oneOf: [
            { const: "whole", description: "The whole slide." },
            {
              type: "object",
              properties: {
                rect: {
                  type: "object",
                  properties: { width: { type: "number" } },
                  required: ["width"],
                },
              },
              required: ["rect"],
              description: "A rectangle.",
            },
          ],
        },
        pattern: {
          oneOf: [
            {
              type: "object",
              properties: { kind: { const: "spiral" }, turns: { type: "integer" } },
              required: ["kind", "turns"],
            },
            { type: "object", properties: { kind: { const: "snake" } }, required: ["kind"] },
          ],
        },
      },
      required: ["region", "pattern"],
    };
    const drawn = form(schema, {
      region: { rect: { width: 5 } },
      pattern: { kind: "spiral", turns: 3 },
    });
    expect(drawn.wrapper.findAll("legend").map((one) => one.text())).toEqual(["Region", "Pattern"]);
    expect(drawn.note("Kind")).toBe("A rectangle.");
    expect(drawn.field("Width").get("input").element).toHaveProperty("value", "5");
    expect(drawn.field("Turns").get("input").element).toHaveProperty("value", "3");
    await type(drawn.field("Width").get("input"), "7", true);
    expect(drawn.now()).toEqual({
      region: { rect: { width: 7 } },
      pattern: { kind: "spiral", turns: 3 },
    });
    // A kind that holds nothing draws nothing more.
    await drawn.set({ modelValue: { region: "whole", pattern: { kind: "snake" } } });
    expect(drawn.wrapper.findAll(".field").map((one) => one.find("label").text())).toEqual([
      "Kind",
      "Kind",
    ]);
  });

  it("adds and takes out a list's items, within its bounds", async () => {
    const drawn = form({
      type: "object",
      properties: {
        heights_um: { type: "array", items: { type: "number" }, minItems: 1, maxItems: 2 },
      },
      required: ["heights_um"],
    });
    expect(drawn.now()).toEqual({ heights_um: [0] });
    expect(drawn.button("Remove Heights (µm) 1")!.attributes("disabled")).toBeDefined();
    await drawn.button("Add to Heights (µm)")!.trigger("click");
    await type(drawn.field("Heights (µm) 2").get("input"), "5", true);
    expect(drawn.now()).toEqual({ heights_um: [0, 5] });
    expect(drawn.button("Add to Heights (µm)")!.attributes("disabled")).toBeDefined();
    await drawn.button("Remove Heights (µm) 1")!.trigger("click");
    expect(drawn.now()).toEqual({ heights_um: [5] });
  });

  it("adds, renames and takes out a map's entries, and refuses a name taken", async () => {
    const drawn = form({
      type: "object",
      properties: { labels: { type: "object", additionalProperties: { type: "string" } } },
      required: ["labels"],
    });
    await drawn.button("Add to Labels")!.trigger("click");
    await type(drawn.field("The name of entry 1").get("input"), "slide");
    await type(drawn.field("slide").get("input"), "4");
    await drawn.button("Add to Labels")!.trigger("click");
    expect(drawn.now()).toEqual({ labels: { slide: "4", "entry 2": "" } });
    await type(drawn.field("The name of entry 2").get("input"), "slide");
    expect(drawn.note("The name of entry 2")).toBe("That name is taken.");
    expect(drawn.now()).toEqual({ labels: { slide: "4", "entry 2": "" } });
    await drawn.button("Remove slide")!.trigger("click");
    expect(drawn.now()).toEqual({ labels: { "entry 2": "" } });
  });

  it("takes JSON written by hand, and says why text isn't JSON", async () => {
    const drawn = form({
      type: "object",
      properties: { extra: { description: "Anything else." } },
      required: ["extra"],
    });
    const text = drawn.field("Extra").get("textarea");
    expect((text.element as HTMLTextAreaElement).value).toBe("null");
    await type(text, '{"a": [1, 2]}');
    expect(drawn.now()).toEqual({ extra: { a: [1, 2] } });
    await type(text, '{"a": ');
    expect(drawn.note("Extra")).toMatch(/^This isn't JSON: /);
    expect(drawn.now()).toEqual({ extra: { a: [1, 2] } });
  });

  it("draws what it doesn't know as JSON, and says why", () => {
    const drawn = form({
      type: "object",
      properties: { slide: { type: "string", pattern: "^[A-Z]{2}$", default: "AB" } },
    });
    expect(drawn.note("Slide")).toBe("Written as JSON: it uses `pattern`.");
    expect(drawn.field("Slide").attributes("data-fallback")).toBe("it uses `pattern`");
    expect((drawn.field("Slide").get("textarea").element as HTMLTextAreaElement).value).toBe(
      '"AB"',
    );
  });

  it("puts the server's refusals on the fields they name, until those change", async () => {
    const drawn = form(scan, { name: "x", exposure_ms: 20, tiles: 16, autofocus: true });
    await drawn.set({
      refused: {
        detail: [
          {
            type: "less_than_equal",
            loc: ["body", "tiles"],
            msg: "Input should be at most 100",
            input: 16,
          },
          { type: "value_error", loc: ["body", "lens"], msg: "No such lens", input: null },
        ],
      },
    });
    expect(drawn.note("Tiles")).toBe("Input should be at most 100");
    // What names no field is the whole form's.
    expect(drawn.wrapper.find(".group > .error").text()).toBe("No such lens");
    expect(drawn.faults.at(-1)).toHaveLength(2);
    await type(drawn.field("Tiles").get("input"), "12", true);
    expect(drawn.note("Tiles")).toBe("");
    expect(drawn.wrapper.find(".group > .error").text()).toBe("No such lens");
  });

  it("is read-only when told: nothing to add, take out or change", () => {
    const drawn = form(
      {
        type: "object",
        properties: {
          corners: { type: "array", items: { type: "number" } },
          name: { type: "string" },
        },
        required: ["corners", "name"],
      },
      { corners: [1], name: "x" },
      { readonly: true },
    );
    expect(drawn.wrapper.findAll("button")).toHaveLength(0);
    expect(
      drawn.wrapper.findAll("input").every((one) => one.attributes("readonly") !== undefined),
    ).toBe(true);
  });

  it("names a value alone by the form's name, and an object with no fields says so", () => {
    expect(form({ type: "integer", minimum: 1 }).field("The scan").find("input").exists()).toBe(
      true,
    );
    expect(form({ type: "object", properties: {} }).wrapper.text()).toBe("Nothing to fill in.");
  });
});
