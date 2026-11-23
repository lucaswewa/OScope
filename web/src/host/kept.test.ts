import { describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";

import type { Store } from "@/host/adapter";
import { browserHost } from "@/host/browser";
import { installHost } from "@/host/host";
import { kept } from "@/host/kept";

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");

describe("a value the browser keeps", () => {
  it("is what was kept, and the fallback where nothing was", () => {
    localStorage.setItem("oscope.test.kept", "[60,40]");
    expect(kept("oscope.test.kept", [70, 30], isPair).value).toEqual([60, 40]);
    expect(kept("oscope.test.other", [70, 30], isPair).value).toEqual([70, 30]);
    localStorage.clear();
  });

  it.each(["[60]", '"60,40"', "not JSON", "null", '[60,"40"]'])(
    "is the fallback where what was kept isn't of its kind: %s",
    (was) => {
      localStorage.setItem("oscope.test.kept", was);
      expect(kept("oscope.test.kept", [70, 30], isPair).value).toEqual([70, 30]);
      localStorage.clear();
    },
  );

  it("keeps each change, made to it or inside it", async () => {
    const sizes = kept("oscope.test.kept", [70, 30] as [number, number], isPair);
    sizes.value = [65, 35];
    await nextTick();
    expect(localStorage.getItem("oscope.test.kept")).toBe("[65,35]");
    sizes.value[0] = 50;
    await nextTick();
    expect(localStorage.getItem("oscope.test.kept")).toBe("[50,35]");
    localStorage.clear();
  });

  it("still changes where the browser keeps nothing", async () => {
    const refused = (): never => {
      throw new DOMException("refused", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: refused, setItem: refused });
    const sizes = kept("oscope.test.kept", [70, 30] as [number, number], isPair);
    expect(sizes.value).toEqual([70, 30]);
    sizes.value = [55, 45];
    await nextTick();
    expect(sizes.value).toEqual([55, 45]);
    vi.unstubAllGlobals();
  });

  it("is kept by the host the app has: the desktop app's, once it installs its own", async () => {
    const values = new Map([["oscope.test.kept", "[10,90]"]]);
    const store: Store = {
      read: (key) => values.get(key),
      write: (key, value) =>
        void (value === undefined ? values.delete(key) : values.set(key, value)),
    };
    installHost({ ...browserHost(window), platform: "desktop", kept: store });
    try {
      const sizes = kept("oscope.test.kept", [70, 30] as [number, number], isPair);
      expect(sizes.value).toEqual([10, 90]);
      sizes.value = [20, 80];
      await nextTick();
      expect(values.get("oscope.test.kept")).toBe("[20,80]");
      expect(localStorage.getItem("oscope.test.kept")).toBeNull();
    } finally {
      installHost(undefined);
    }
  });
});
