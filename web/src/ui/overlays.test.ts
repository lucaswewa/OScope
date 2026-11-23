import { flushPromises } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { h, nextTick, ref, type Component } from "vue";

import Confirm from "@/ui/Confirm.vue";
import Dialog from "@/ui/Dialog.vue";
import { draw } from "@/ui/test-kit";
import TextField from "@/ui/TextField.vue";

/** Lets the library's layers settle: focus moves a frame after opening. */
async function settled(): Promise<void> {
  await flushPromises();
  await new Promise((done) => setTimeout(done, 20));
  await nextTick();
}

const press = (key: string, target: Element | Document = document): void => {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
};

/** A page with a button that opens `overlay`, and what the overlay said. */
function page(overlay: Component, props: Record<string, unknown>, inside?: () => unknown) {
  const open = ref(false);
  const said: string[] = [];
  const drawn = draw({
    render: () => [
      h("button", { id: "opener", onClick: () => (open.value = true) }, "Open"),
      h(
        overlay,
        {
          ...props,
          open: open.value,
          "onUpdate:open": (now: boolean) => (open.value = now),
          onConfirm: () => said.push("confirm"),
          onCancel: () => said.push("cancel"),
        },
        inside ? { default: inside } : {},
      ),
    ],
  });
  const opener = document.getElementById("opener") as HTMLButtonElement;
  return {
    ...drawn,
    open,
    said,
    async show(): Promise<void> {
      opener.focus();
      open.value = true;
      await settled();
    },
    opener,
    focused: () => document.activeElement,
  };
}

describe("a dialog", () => {
  const props = { title: "A new scan", description: "Its name can be changed later." };
  const named = () => h(TextField, { label: "Name" });

  it("takes the focus to the first thing in it, and says what it is", async () => {
    const scan = page(Dialog, props, named);
    await scan.show();
    const dialog = document.querySelector("[role='dialog']")!;
    expect(dialog).not.toBeNull();
    expect(scan.focused()).toBe(dialog.querySelector("input"));
    expect(dialog.getAttribute("aria-labelledby")).toBe(
      dialog.querySelector(".ui-dialog-title")!.id,
    );
    expect(dialog.getAttribute("aria-describedby")).toBe(
      dialog.querySelector(".ui-dialog-text")!.id,
    );
    expect(document.querySelector(".ui-scrim")).not.toBeNull();
  });

  it("keeps the focus inside while it is open", async () => {
    const scan = page(Dialog, props, named);
    await scan.show();
    const field = scan.focused();
    // Sent outside, as a press or a script might: it comes back.
    scan.opener.focus();
    await settled();
    expect(scan.focused()).toBe(field);
  });

  it("closes on Escape, and gives the focus back to what opened it", async () => {
    const scan = page(Dialog, props, named);
    await scan.show();
    press("Escape", scan.focused()!);
    await settled();
    expect(scan.open.value).toBe(false);
    expect(document.querySelector("[role='dialog']")).toBeNull();
    expect(scan.focused()).toBe(scan.opener);
  });

  it("closes by its own button, named in the components' words", async () => {
    const scan = page(Dialog, props);
    await scan.show();
    const close = document.querySelector<HTMLButtonElement>(".ui-dialog-close button")!;
    expect(close.getAttribute("aria-label")).toBe("Close");
    close.click();
    await settled();
    expect(scan.open.value).toBe(false);
  });

  it("says nothing more than its title where it has no description", async () => {
    const plain = page(Dialog, { title: "Options" });
    await plain.show();
    const dialog = document.querySelector("[role='dialog']")!;
    expect(dialog.querySelector(".ui-dialog-text")).toBeNull();
    expect(dialog.hasAttribute("aria-describedby")).toBe(false);
  });
});

describe("a confirmation", () => {
  const props = {
    title: "Stop the scan?",
    text: "The tiles taken so far are kept.",
    confirm: "Stop the scan",
    cancel: "Keep scanning",
  };
  const button = (name: string): HTMLButtonElement =>
    [...document.querySelectorAll<HTMLButtonElement>("[role='alertdialog'] button")].find(
      (one) => one.textContent?.trim() === name,
    )!;

  it("opens with the focus on the answer that changes nothing", async () => {
    const question = page(Confirm, props);
    await question.show();
    expect(document.querySelector("[role='alertdialog']")).not.toBeNull();
    expect(question.focused()).toBe(button("Keep scanning"));
  });

  it("says yes by its button, no by the other, and no by Escape", async () => {
    const question = page(Confirm, props);
    await question.show();
    button("Stop the scan").click();
    await settled();
    expect(question.said).toEqual(["confirm"]);
    expect(question.open.value).toBe(false);

    await question.show();
    button("Keep scanning").click();
    await settled();
    await question.show();
    press("Escape", question.focused()!);
    await settled();
    expect(question.said).toEqual(["confirm", "cancel", "cancel"]);
    expect(question.focused()).toBe(question.opener);
  });

  it("goes on only once its word is typed, with the focus in the field to type it", async () => {
    const deletion = page(Confirm, { ...props, confirm: "Delete", typed: "slide-4" });
    await deletion.show();
    const field = document.querySelector<HTMLInputElement>("[role='alertdialog'] input")!;
    expect(deletion.focused()).toBe(field);
    expect(document.querySelector("[role='alertdialog'] label")!.textContent).toBe(
      "Type slide-4 to go on",
    );
    const go = (): HTMLButtonElement => button("Delete");
    expect(go().disabled).toBe(true);

    for (const typed of ["slide", "slide-44", "Slide-4"]) {
      field.value = typed;
      field.dispatchEvent(new Event("input"));
      await nextTick();
      expect(go().disabled, typed).toBe(true);
    }
    field.value = " slide-4 ";
    field.dispatchEvent(new Event("input"));
    await nextTick();
    expect(go().disabled).toBe(false);
    go().click();
    await settled();
    expect(deletion.said).toEqual(["confirm"]);

    // The next time, the word must be typed again.
    await deletion.show();
    expect(document.querySelector<HTMLInputElement>("[role='alertdialog'] input")!.value).toBe("");
    expect(go().disabled).toBe(true);
  });

  it("says no by its other button, though the library doesn't know that one", async () => {
    const deletion = page(Confirm, { ...props, confirm: "Delete", typed: "slide-4" });
    await deletion.show();
    button("Keep scanning").click();
    await settled();
    expect(deletion.said).toEqual(["cancel"]);
    expect(deletion.open.value).toBe(false);
    expect(deletion.focused()).toBe(deletion.opener);
  });
});
