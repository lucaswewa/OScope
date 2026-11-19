import { describe, expect, it } from "vitest";

import { handClock } from "@/ui/test-kit";
import { AT_ONCE, createToasts, KEPT, lasting, LASTS, type Level } from "@/ui/toasts";

const texts = (queue: ReturnType<typeof createToasts>): string[] =>
  queue.shown.value.map((toast) => toast.text);

/** A queue with notices "1" to `count` raised in turn, and the numbers they go by. */
function raised(count: number, atOnce?: number) {
  const queue = createToasts(atOnce);
  const ids = Array.from({ length: count }, (_, at) =>
    queue.notify({ level: "info", text: String(at + 1) }),
  );
  return { queue, ids };
}

describe("the notices' queue", () => {
  it("shows the oldest few, and keeps the others waiting", () => {
    const { queue } = raised(5);
    expect(AT_ONCE).toBe(3);
    expect(texts(queue)).toEqual(["1", "2", "3"]);
    expect(queue.waiting.value).toBe(2);

    const few = raised(2).queue;
    expect(texts(few)).toEqual(["1", "2"]);
    expect(few.waiting.value).toBe(0);
    expect(texts(createToasts())).toEqual([]);
  });

  it("gives the place of one that goes to the next that was raised", () => {
    const { queue, ids } = raised(5);
    queue.dismiss(ids[1]!);
    expect(texts(queue)).toEqual(["1", "3", "4"]);
    expect(queue.waiting.value).toBe(1);
    queue.dismiss(ids[0]!);
    expect(texts(queue)).toEqual(["3", "4", "5"]);
    expect(queue.waiting.value).toBe(0);
    queue.dismiss(ids[2]!);
    expect(texts(queue)).toEqual(["4", "5"]);
  });

  it("lets a notice that is still waiting be sent away, and one already gone", () => {
    const { queue, ids } = raised(5);
    queue.dismiss(ids[4]!);
    expect(texts(queue)).toEqual(["1", "2", "3"]);
    expect(queue.waiting.value).toBe(1);
    queue.dismiss(ids[4]!);
    queue.dismiss(999);
    expect(texts(queue)).toEqual(["1", "2", "3"]);
    expect(queue.waiting.value).toBe(1);
  });

  it("gives each notice a number of its own, never used again", () => {
    const { queue, ids } = raised(3);
    expect(new Set(ids).size).toBe(3);
    queue.dismiss(ids[2]!);
    const next = queue.notify({ level: "success", text: "4" });
    expect(ids).not.toContain(next);
    expect(queue.shown.value.at(-1)).toMatchObject({ id: next, level: "success", text: "4" });
  });

  it("keeps a notice's action with it", () => {
    const queue = createToasts();
    let undone = 0;
    queue.notify({
      level: "success",
      text: "Saved",
      action: { label: "Undo", run: () => undone++ },
    });
    queue.shown.value[0]!.action!.run();
    expect(undone).toBe(1);
  });

  it("shows as many at once as it is made for", () => {
    const { queue } = raised(4, 1);
    expect(texts(queue)).toEqual(["1"]);
    expect(queue.waiting.value).toBe(3);
  });

  it("keeps two queues apart", () => {
    const one = raised(1).queue;
    const other = createToasts();
    expect(texts(one)).toEqual(["1"]);
    expect(texts(other)).toEqual([]);
  });
});

describe("the notices' history (W2.2)", () => {
  it("keeps every notice raised, the newest first, sent away or not", () => {
    const { queue, ids } = raised(5);
    queue.dismiss(ids[0]!);
    queue.dismiss(ids[3]!);
    expect(queue.history.value.map((told) => told.text)).toEqual(["5", "4", "3", "2", "1"]);
    expect(queue.history.value.map((told) => told.id)).toEqual([...ids].reverse());
  });

  it("keeps when each was raised", () => {
    const before = Date.now();
    const { queue } = raised(1);
    const at = queue.history.value[0]!.at.getTime();
    expect(at).toBeGreaterThanOrEqual(before);
    expect(at).toBeLessThanOrEqual(Date.now());
  });

  it("keeps the newest KEPT, and lets the oldest go", () => {
    const { queue } = raised(KEPT + 5);
    expect(queue.history.value).toHaveLength(KEPT);
    expect(queue.history.value[0]!.text).toBe(String(KEPT + 5));
    expect(queue.history.value.at(-1)!.text).toBe("6");
  });
});

describe("how long a notice stays", () => {
  it("is its level's time: what failed stays until it is sent away", () => {
    const of = (level: Level): number => lasting({ level, text: "" });
    expect(of("info")).toBe(5000);
    expect(of("success")).toBe(5000);
    expect(of("warning")).toBeGreaterThan(of("info"));
    expect(of("danger")).toBe(0);
    expect(Object.keys(LASTS).sort()).toEqual(["danger", "info", "success", "warning"]);
  });

  it("is its own time where it gives one", () => {
    expect(lasting({ level: "danger", text: "", lasts: 1500 })).toBe(1500);
    expect(lasting({ level: "info", text: "", lasts: 0 })).toBe(0);
  });
});

describe("a notice's time", () => {
  /** A queue on a hand-turned clock, and a notice in it. */
  function timed(atOnce = AT_ONCE) {
    const hand = handClock();
    const queue = createToasts(atOnce, hand.clock);
    return { ...hand, queue, texts: () => texts(queue) };
  }

  it("runs out, and the notice goes", () => {
    const { queue, pass, texts: now } = timed();
    queue.notify({ level: "info", text: "Saved" });
    pass(4999);
    expect(now()).toEqual(["Saved"]);
    pass(1);
    expect(now()).toEqual([]);
  });

  it("never runs out for what failed, nor for one given no end", () => {
    const { queue, pass, texts: now, running } = timed();
    queue.notify({ level: "danger", text: "Failed" });
    queue.notify({ level: "info", text: "Kept", lasts: 0 });
    pass(24 * 3600 * 1000);
    expect(now()).toEqual(["Failed", "Kept"]);
    expect(running()).toBe(0);
  });

  it("runs only while the notice shows: one that waited gets its whole time", () => {
    const { queue, pass, texts: now } = timed(1);
    queue.notify({ level: "info", text: "first" });
    queue.notify({ level: "info", text: "second" });
    pass(4000);
    expect(now()).toEqual(["first"]);
    pass(1000);
    expect(now()).toEqual(["second"]);
    pass(4999);
    expect(now()).toEqual(["second"]);
    pass(1);
    expect(now()).toEqual([]);
  });

  it("stops while paused, and goes on from where it stopped", () => {
    const { queue, pass, texts: now, running } = timed();
    queue.notify({ level: "info", text: "Saved" });
    pass(3000);
    queue.pause();
    expect(running()).toBe(0);
    pass(60000);
    expect(now()).toEqual(["Saved"]);
    queue.resume();
    pass(1999);
    expect(now()).toEqual(["Saved"]);
    pass(1);
    expect(now()).toEqual([]);
  });

  it("doesn't start for a notice raised while paused, until it is resumed", () => {
    const { queue, pass, texts: now } = timed();
    queue.pause();
    queue.notify({ level: "success", text: "Saved", lasts: 1500 });
    pass(10000);
    expect(now()).toEqual(["Saved"]);
    queue.resume();
    pass(1500);
    expect(now()).toEqual([]);
  });

  it("is stopped and started once, however often it is asked", () => {
    const { queue, pass, texts: now } = timed();
    queue.notify({ level: "info", text: "Saved" });
    pass(1000);
    queue.pause();
    queue.pause();
    pass(1000);
    queue.resume();
    queue.resume();
    pass(3999);
    expect(now()).toEqual(["Saved"]);
    pass(1);
    expect(now()).toEqual([]);
  });

  it("is forgotten with its notice, so that a notice sent away leaves no timer", () => {
    const { queue, running } = timed();
    const id = queue.notify({ level: "warning", text: "Nearly full" });
    expect(running()).toBe(1);
    queue.dismiss(id);
    expect(running()).toBe(0);
  });
});
