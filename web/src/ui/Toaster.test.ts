import { describe, expect, it } from "vitest";
import { h, nextTick } from "vue";

import { draw, handClock } from "@/ui/test-kit";
import Toaster from "@/ui/Toaster.vue";
import { createToasts, type Notice } from "@/ui/toasts";

/** A page with a button, and the notices of a queue on a hand-turned clock. */
function page() {
  const hand = handClock();
  const queue = createToasts(3, hand.clock);
  const drawn = draw({
    render: () => [
      h("button", { id: "work" }, "Work"),
      h(Toaster, { toasts: queue, "data-test": "mine" }),
    ],
  });
  // The provider draws the app's own notices too: these are the queue's.
  const region = document.querySelector<HTMLElement>(".ui-toasts-region[data-test='mine']")!;
  return {
    ...drawn,
    ...hand,
    queue,
    region,
    list: region.querySelector<HTMLElement>(".ui-toasts")!,
    work: document.getElementById("work") as HTMLButtonElement,
    async raise(...notices: Notice[]): Promise<void> {
      for (const notice of notices) queue.notify(notice);
      await nextTick();
    },
    shown: () => [...region.querySelectorAll(".ui-toast-text")].map((one) => one.textContent),
    said: (role: string) =>
      [...region.querySelectorAll(`[role='${role}'] p`)].map((one) => one.textContent),
    focused: () => document.activeElement,
  };
}

const key = (target: Element | Window, name: string): KeyboardEvent => {
  const event = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
};

describe("the notices on the page", () => {
  it("are in a region named for the key that reaches them", () => {
    const { region } = page();
    expect(region.tagName).toBe("SECTION");
    expect(region.getAttribute("aria-label")).toBe("Notifications (F8)");
  });

  it("are read out as they show: at once if they warn or failed, else when the reader is free", async () => {
    const notices = page();
    // The regions are there before anything is said in them.
    expect(notices.region.querySelector("[role='status']")).not.toBeNull();
    expect(notices.region.querySelector("[role='alert']")).not.toBeNull();
    await notices.raise(
      { level: "info", text: "Homed." },
      { level: "danger", text: "The camera stopped." },
      { level: "success", text: "Saved." },
      { level: "warning", text: "Nearly full." },
    );
    expect(notices.shown()).toEqual(["Homed.", "The camera stopped.", "Saved."]);
    expect(notices.said("status")).toEqual(["Notification: Homed.", "Notification: Saved."]);
    expect(notices.said("alert")).toEqual(["Notification: The camera stopped."]);

    // One that waited is read out when it shows.
    notices.queue.dismiss(notices.queue.shown.value[0]!.id);
    await nextTick();
    expect(notices.said("alert")).toEqual([
      "Notification: The camera stopped.",
      "Notification: Nearly full.",
    ]);
  });

  it("each have their icon, and a button to send them away", async () => {
    const notices = page();
    await notices.raise({ level: "warning", text: "Nearly full." });
    const item = notices.region.querySelector(".ui-toast")!;
    expect(item.classList).toContain("warning");
    expect(item.querySelector("svg")!.getAttribute("aria-hidden")).toBe("true");
    item.querySelector<HTMLButtonElement>("button[aria-label='Dismiss']")!.click();
    await nextTick();
    expect(notices.shown()).toEqual([]);
  });

  it("do what a notice offers, and the notice goes", async () => {
    const notices = page();
    let undone = 0;
    await notices.raise({
      level: "success",
      text: "Deleted.",
      action: { label: "Undo", run: () => undone++ },
    });
    const undo = [...notices.region.querySelectorAll("button")].find(
      (one) => one.textContent?.trim() === "Undo",
    )!;
    undo.click();
    await nextTick();
    expect(undone).toBe(1);
    expect(notices.shown()).toEqual([]);
  });

  it("stop their time while the pointer is on them, or the keyboard's focus", async () => {
    const notices = page();
    await notices.raise({ level: "info", text: "Homed." });
    notices.region.dispatchEvent(new PointerEvent("pointerenter"));
    await nextTick();
    notices.pass(60000);
    expect(notices.shown()).toEqual(["Homed."]);
    notices.region.dispatchEvent(new PointerEvent("pointerleave"));
    await nextTick();

    notices.region.querySelector<HTMLButtonElement>(".ui-toast button")!.focus();
    await nextTick();
    notices.pass(60000);
    expect(notices.shown()).toEqual(["Homed."]);

    // The focus moving among them keeps them; leaving them lets their time run.
    notices.list.focus();
    await nextTick();
    notices.pass(60000);
    expect(notices.shown()).toEqual(["Homed."]);
    notices.work.focus();
    await nextTick();
    notices.pass(5000);
    await nextTick();
    expect(notices.shown()).toEqual([]);
  });

  it("take the focus on F8, and give it back on Escape", async () => {
    const notices = page();
    // With nothing to show, F8 is left alone.
    notices.work.focus();
    expect(key(window, "F8").defaultPrevented).toBe(false);
    expect(notices.focused()).toBe(notices.work);

    await notices.raise({ level: "danger", text: "Failed." }, { level: "info", text: "Homed." });
    expect(key(window, "F8").defaultPrevented).toBe(true);
    expect(notices.focused()).toBe(notices.list);
    // Pressed again there, it stays, and still knows where to go back to.
    key(window, "F8");
    key(notices.list, "Escape");
    await nextTick();
    expect(notices.focused()).toBe(notices.work);
    expect(notices.shown()).toEqual(["Failed.", "Homed."]);
  });

  it("send away the one with the focus on Escape, passing the focus on, and back at the last", async () => {
    const notices = page();
    await notices.raise({ level: "danger", text: "Failed." }, { level: "info", text: "Homed." });
    notices.work.focus();
    key(window, "F8");
    const first = notices.region.querySelector<HTMLButtonElement>(".ui-toast button")!;
    first.focus();
    key(first, "Escape");
    await nextTick();
    expect(notices.shown()).toEqual(["Homed."]);
    expect(notices.focused()).toBe(notices.list);

    const last = notices.region.querySelector<HTMLButtonElement>(".ui-toast button")!;
    last.focus();
    key(last, "Escape");
    await nextTick();
    expect(notices.shown()).toEqual([]);
    expect(notices.focused()).toBe(notices.work);
  });

  it("leave every other Escape alone: closing a menu sends no notice away", async () => {
    const notices = page();
    await notices.raise({ level: "danger", text: "Failed." });
    notices.work.focus();
    const elsewhere = key(notices.work, "Escape");
    key(window, "Escape");
    await nextTick();
    expect(notices.shown()).toEqual(["Failed."]);
    expect(elsewhere.defaultPrevented).toBe(false);
  });
});
