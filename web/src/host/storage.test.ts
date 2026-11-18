import { afterEach, describe, expect, it, vi } from "vitest";

import { read, write } from "@/host/storage";

describe("the browser's storage", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("keeps a value under its name, and nothing under another", () => {
    write("oscope.test", "kept");
    expect(read("oscope.test")).toBe("kept");
    expect(read("oscope.other")).toBeNull();
  });

  it("keeps nothing, and says nothing, when the browser refuses", () => {
    const refused = (): never => {
      throw new DOMException("refused", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: refused, setItem: refused });
    expect(() => write("oscope.test", "lost")).not.toThrow();
    expect(read("oscope.test")).toBeNull();
    vi.unstubAllGlobals();
    expect(read("oscope.test")).toBeNull();
  });
});
