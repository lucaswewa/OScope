import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useDisplay } from "@/features/display/display";

/** The computer's wish for a dark scheme, which a test can turn. */
function computer(dark: boolean) {
  const list = Object.assign(new EventTarget(), { matches: dark });
  return {
    list: list as unknown as MediaQueryList,
    turn(to: boolean): void {
      list.matches = to;
      list.dispatchEvent(Object.assign(new Event("change"), { matches: to }));
    },
  };
}

/** A visit: a page's root, and the store started on it. */
function visit(dark = false) {
  setActivePinia(createPinia());
  const root = document.createElement("html");
  const system = computer(dark);
  const display = useDisplay();
  display.start(root, system.list);
  return { root, system, display };
}

const keptNow = (): unknown => JSON.parse(localStorage.getItem("oscope.display") ?? "null");

describe("the display's theme and accent", () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.unstubAllGlobals());

  it("start as the system's theme, in magenta: no theme on the root at all", () => {
    const { root, display } = visit();
    expect([display.theme, display.accent]).toEqual(["system", "magenta"]);
    expect(root.hasAttribute("data-theme")).toBe(false);
    expect(root.getAttribute("data-accent")).toBe("magenta");
    // Nothing is kept until something is chosen.
    expect(keptNow()).toBeNull();
  });

  it("reach the root at once when changed, and are there on the next visit", () => {
    const first = visit();
    first.display.theme = "dark";
    expect(first.root.getAttribute("data-theme")).toBe("dark");
    first.display.accent = "fern";
    expect(first.root.getAttribute("data-accent")).toBe("fern");
    expect(keptNow()).toEqual({ theme: "dark", accent: "fern" });

    const next = visit();
    expect([next.display.theme, next.display.accent]).toEqual(["dark", "fern"]);
    expect(next.root.getAttribute("data-theme")).toBe("dark");
    expect(next.root.getAttribute("data-accent")).toBe("fern");

    // Back to the system's: the attribute goes, so the page follows the computer.
    next.display.theme = "system";
    expect(next.root.hasAttribute("data-theme")).toBe(false);
  });

  it("give the scheme in use: the computer's under system, as it changes", () => {
    const { display, system } = visit(true);
    expect(display.scheme).toBe("dark");
    system.turn(false);
    expect(display.scheme).toBe("light");
    system.turn(true);
    expect(display.scheme).toBe("dark");
    // A theme that was chosen holds, whatever the computer does.
    display.theme = "light";
    expect(display.scheme).toBe("light");
    system.turn(false);
    system.turn(true);
    expect(display.scheme).toBe("light");
    display.theme = "dark";
    expect(display.scheme).toBe("dark");
  });

  it.each([
    ['{"theme":"sepia","accent":"gold"}', "system", "magenta"],
    ['{"theme":"dark"}', "dark", "magenta"],
    ['{"accent":"harbor","more":1}', "system", "harbor"],
    ['{"theme":3,"accent":null}', "system", "magenta"],
    ["not JSON", "system", "magenta"],
    ["[1]", "system", "magenta"],
    ["null", "system", "magenta"],
  ])("take what was kept only where the app offers it: %s", (was, theme, accent) => {
    localStorage.setItem("oscope.display", was);
    const { display, root } = visit();
    expect([display.theme, display.accent]).toEqual([theme, accent]);
    expect(root.getAttribute("data-accent")).toBe(accent);
  });

  it("change even when the browser refuses to keep them", () => {
    const refused = (): never => {
      throw new DOMException("refused", "SecurityError");
    };
    vi.stubGlobal("localStorage", { getItem: refused, setItem: refused });
    const { display, root } = visit();
    display.theme = "dark";
    display.accent = "dusk";
    expect(root.getAttribute("data-theme")).toBe("dark");
    expect(root.getAttribute("data-accent")).toBe("dusk");
  });
});
