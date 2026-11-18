import { afterEach, describe, expect, it } from "vitest";
import { h } from "vue";

import Checkbox from "@/ui/Checkbox.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import RadioGroup from "@/ui/RadioGroup.vue";
import Slider from "@/ui/Slider.vue";
import { draw, drawModel } from "@/ui/test-kit";
import TextArea from "@/ui/TextArea.vue";
import TextField from "@/ui/TextField.vue";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("a text field", () => {
  it("holds what is typed, at each key", async () => {
    const name = drawModel(TextField, "", { placeholder: "lab-pc:8210" });
    const input = name.wrapper.get("input");
    expect(input.attributes("placeholder")).toBe("lab-pc:8210");
    await input.setValue("s");
    await input.setValue("slide 4");
    expect(name.changes).toEqual(["s", "slide 4"]);
    expect((input.element as HTMLInputElement).value).toBe("slide 4");
  });

  it("shows a value from outside", async () => {
    const name = drawModel(TextField, "one");
    await name.set({ modelValue: "two" });
    expect((name.wrapper.get("input").element as HTMLInputElement).value).toBe("two");
  });

  it("says what it is when read-only, switched off, in error, or named", () => {
    const of = (props: Record<string, unknown>) => draw(TextField, props).wrapper.get("input");
    expect(of({ readonly: true }).attributes("readonly")).toBeDefined();
    expect(of({ disabled: true }).attributes("disabled")).toBeDefined();
    expect(of({ invalid: true }).attributes("aria-invalid")).toBe("true");
    expect(of({}).attributes("aria-invalid")).toBeUndefined();
    expect(of({ label: "Sample ID" }).attributes("aria-label")).toBe("Sample ID");
    expect(of({ compact: true }).classes()).toContain("compact");
  });
});

describe("a text area", () => {
  it("holds several lines, and shows as many as asked", async () => {
    const notes = drawModel(TextArea, "", { rows: 5 });
    const area = notes.wrapper.get("textarea");
    expect(area.attributes("rows")).toBe("5");
    await area.setValue("one\ntwo");
    expect(notes.changes).toEqual(["one\ntwo"]);
    expect(
      draw(TextArea, { readonly: true }).wrapper.get("textarea").attributes("readonly"),
    ).toBeDefined();
  });
});

describe("a field", () => {
  /** A field around a control, as a page writes it. */
  const around = (control: () => unknown, props: Record<string, unknown> = {}) =>
    draw(Field, { label: "Sample ID", ...props }, { default: control });

  it("names the control inside it, with nothing wired by hand", () => {
    const { wrapper } = around(() => h(TextField));
    const label = wrapper.get("label");
    const input = wrapper.get("input");
    expect(label.text()).toBe("Sample ID");
    expect(input.attributes("id")).toBeTruthy();
    expect(label.attributes("for")).toBe(input.attributes("id"));
    expect(input.attributes("aria-describedby")).toBeUndefined();
    expect(input.attributes("aria-invalid")).toBeUndefined();
  });

  it("describes it by its hint, and by its error in the hint's place", async () => {
    const drawn = around(() => h(TextField), { hint: "Names the scan." });
    const input = drawn.wrapper.get("input");
    const note = () => drawn.wrapper.get("p");
    expect(note().text()).toBe("Names the scan.");
    expect(input.attributes("aria-describedby")).toBe(note().attributes("id"));
    expect(input.attributes("aria-invalid")).toBeUndefined();

    await drawn.set({ error: "This name is taken." });
    expect(drawn.wrapper.findAll("p")).toHaveLength(1);
    expect(note().text()).toBe("This name is taken.");
    expect(note().classes()).toContain("error");
    expect(input.attributes("aria-describedby")).toBe(note().attributes("id"));
    expect(input.attributes("aria-invalid")).toBe("true");

    await drawn.set({ error: undefined, hint: undefined });
    expect(drawn.wrapper.find("p").exists()).toBe(false);
    expect(input.attributes("aria-describedby")).toBeUndefined();
  });

  it("gives two fields on a page their own ids", () => {
    const second = () => h(Field, { label: "Operator", hint: "Who ran it." }, () => h(TextField));
    const { wrapper } = around(() => [h(TextField), second()], { hint: "Names the scan." });
    const ids = [
      ...wrapper.findAll("input").map((input) => input.attributes("id")),
      ...wrapper.findAll("label").map((label) => label.attributes("id")),
      ...wrapper.findAll("p").map((note) => note.attributes("id")),
    ];
    expect(ids).toHaveLength(6);
    expect(new Set(ids).size).toBe(6);
    // Each label still points at its own control.
    const [outer, inner] = wrapper.findAll("label");
    expect(inner!.attributes("for")).toBe(wrapper.findAll("input")[1]!.attributes("id"));
    expect(outer!.attributes("for")).not.toBe(inner!.attributes("for"));
  });

  it("names a number field, whose own reason joins the field's hint", async () => {
    const drawn = around(() => h(NumberField, { modelValue: 1, min: 0 }), { hint: "In µm." });
    const input = drawn.wrapper.get("input");
    expect(drawn.wrapper.get("label").attributes("for")).toBe(input.attributes("id"));
    (input.element as HTMLInputElement).focus();
    await input.setValue("-3");
    await input.trigger("keydown", { key: "Enter" });
    const reason = drawn.wrapper.get("[role='alert']");
    const hint = drawn.wrapper.get(".note");
    expect(input.attributes("aria-describedby")).toBe(
      `${reason.attributes("id")} ${hint.attributes("id")}`,
    );
  });

  it("names a group by its label's id, since a group can't be pointed at", () => {
    const options = [{ value: "a", label: "A" }];
    const group = around(() => h(RadioGroup, { options })).wrapper;
    expect(group.get("[role='radiogroup']").attributes("aria-labelledby")).toBe(
      group.get("label.label").attributes("id"),
    );
    // A slider's thumb, and the number beside it, both go by the field's label.
    const slider = around(() => h(Slider, { modelValue: 5 })).wrapper;
    const label = slider.get("label.label");
    expect(slider.get("[role='slider']").attributes("aria-labelledby")).toBe(
      label.attributes("id"),
    );
    expect(label.attributes("for")).toBe(slider.get("[role='spinbutton']").attributes("id"));
  });

  it("leaves a control outside any field with an id of its own", () => {
    const box = draw(Checkbox, {}, { default: () => "Invert x" }).wrapper.get("[role='checkbox']");
    expect(box.attributes("id")).toBeTruthy();
    expect(draw(TextField, { id: "mine" }).wrapper.get("input").attributes("id")).toBe("mine");
  });
});
