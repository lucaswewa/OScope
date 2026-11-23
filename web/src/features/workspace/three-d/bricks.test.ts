// The bricks kept (W7.8): up to their budget of bytes, the least recently
// used let go first, a brick larger than the budget never kept, and each
// fetched only once while it is kept.
import { describe, expect, it } from "vitest";

import { Bricks } from "./bricks";

/** A brick of `bytes`, named so that tests can tell which is which. */
const brick = (name: string, bytes: number) => ({ name, samples: new Uint8Array(bytes) });

describe("the bricks kept", () => {
  it("lets the least recently used go when its budget is passed, however many it holds", () => {
    const bricks = new Bricks<ReturnType<typeof brick>>(1_000);
    bricks.put("a", brick("a", 400));
    bricks.put("b", brick("b", 400));
    expect(bricks.get("a")?.name).toBe("a");
    bricks.put("c", brick("c", 400));
    // "b" was used least recently: "a" was read since.
    expect(bricks.get("b")).toBeUndefined();
    expect(bricks.get("a")?.name).toBe("a");
    expect(bricks.size).toEqual({ bytes: 800, bricks: 2 });
    // Exactly the budget is kept; one byte more lets the oldest go.
    bricks.put("d", brick("d", 200));
    expect(bricks.size).toEqual({ bytes: 1_000, bricks: 3 });
    bricks.put("e", brick("e", 1));
    expect(bricks.get("c")).toBeUndefined();
    expect(bricks.size).toEqual({ bytes: 601, bricks: 3 });
    // Many small ones, to well past the budget: it holds no more than it.
    for (let i = 0; i < 100; i++) bricks.put(`s${i}`, brick(`s${i}`, 30));
    expect(bricks.size.bytes).toBeLessThanOrEqual(1_000);
    expect(bricks.size.bricks).toBe(33);
    expect(bricks.get("s99")?.name).toBe("s99");
    expect(bricks.get("s66")).toBeUndefined();
  });

  it("doesn't keep a brick larger than its budget, nor let the others go for it", () => {
    const bricks = new Bricks<ReturnType<typeof brick>>(1_000);
    bricks.put("a", brick("a", 600));
    bricks.put("huge", brick("huge", 1_001));
    expect(bricks.get("huge")).toBeUndefined();
    expect(bricks.get("a")?.name).toBe("a");
  });

  it("counts a brick put again once, at its new size", () => {
    const bricks = new Bricks<ReturnType<typeof brick>>(1_000);
    bricks.put("a", brick("a", 600));
    bricks.put("a", brick("a2", 300));
    expect(bricks.size).toEqual({ bytes: 300, bricks: 1 });
    expect(bricks.get("a")?.name).toBe("a2");
  });

  it("fetches a brick once while it is kept, and again once it was let go", async () => {
    const bricks = new Bricks<ReturnType<typeof brick>>(1_000);
    let fetches = 0;
    const fetch = (name: string) => async () => (fetches++, brick(name, 600));
    expect((await bricks.of("a", fetch("a"))).name).toBe("a");
    expect((await bricks.of("a", fetch("again"))).name).toBe("a");
    expect(fetches).toBe(1);
    await bricks.of("b", fetch("b"));
    expect((await bricks.of("a", fetch("a, again"))).name).toBe("a, again");
    expect(fetches).toBe(3);
  });
});
