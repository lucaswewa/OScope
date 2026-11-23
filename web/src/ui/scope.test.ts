// The read-only mode's reach (W2.1, ADR-0048): a locked part of the page
// switches off every control inside it, a part inside may free them again,
// and they come back when it is unlocked.
import { h } from "vue";
import { describe, expect, it } from "vitest";

import Button from "./Button.vue";
import Checkbox from "./Checkbox.vue";
import ControlScope from "./ControlScope.vue";
import NumberField from "./NumberField.vue";
import Switch from "./Switch.vue";
import { draw } from "./test-kit";
import TextField from "./TextField.vue";

describe("a part of the page whose controls are locked", () => {
  it("switches off each control inside it, but where a part frees them", async () => {
    const drawn = draw(
      ControlScope,
      { locked: true },
      {
        default: () => [
          h(Button, { class: "move" }, () => "Move"),
          h(TextField, { label: "Name", modelValue: "" }),
          h(NumberField, { label: "x", modelValue: 1 }),
          h(Checkbox, { label: "Invert x", modelValue: false }),
          h(Switch, { label: "Lit", modelValue: false }),
          h(ControlScope, { locked: false }, () => h(Switch, { label: "Dark", modelValue: false })),
        ],
      },
    );
    const off = () =>
      drawn.wrapper
        .findAll("button, input, [role='switch'], [role='checkbox']")
        .map((one) => [
          one.attributes("aria-label") ?? one.text(),
          one.attributes("disabled") !== undefined,
        ]);
    expect(off()).toEqual(
      expect.arrayContaining([
        ["Move", true],
        ["Name", true],
        ["Lit", true],
        ["Dark", false],
      ]),
    );
    expect(drawn.wrapper.get("button.move").attributes("disabled")).toBeDefined();
    await drawn.set({ locked: false });
    expect(drawn.wrapper.get("button.move").attributes("disabled")).toBeUndefined();
    expect(off().every(([, disabled]) => !disabled)).toBe(true);
  });
});
