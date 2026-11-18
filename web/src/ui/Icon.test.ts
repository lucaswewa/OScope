import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import Icon from "@/ui/Icon.vue";
import { BOX, icons, pathOf, type IconName } from "@/ui/icons";

describe("an icon", () => {
  it("is drawn from the registry by its name, and is silent beside text", () => {
    const svg = mount(Icon, { props: { name: "close" } }).get("svg");
    expect(svg.attributes("viewBox")).toBe(BOX);
    expect(svg.get("path").attributes("d")).toBe(icons.close);
    expect(svg.attributes("aria-hidden")).toBe("true");
    expect(svg.attributes("role")).toBeUndefined();
    expect(svg.attributes("aria-label")).toBeUndefined();
    expect(svg.classes()).toEqual(["icon"]);
  });

  it("is an image with a name when it stands alone", () => {
    const svg = mount(Icon, { props: { name: "danger", label: "Failed", small: true } }).get("svg");
    expect(svg.attributes("role")).toBe("img");
    expect(svg.attributes("aria-label")).toBe("Failed");
    expect(svg.attributes("aria-hidden")).toBeUndefined();
    expect(svg.classes()).toContain("small");
  });

  it("is one path each, none like another", () => {
    const paths = Object.values(icons);
    expect(paths.length).toBeGreaterThanOrEqual(30);
    for (const [name, d] of Object.entries(icons)) expect(d, name).toMatch(/^[Mm][-\d.]/);
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("is refused when its file isn't in the form the icons have", () => {
    const file = (inside: string, box = BOX): string => `<svg viewBox="${box}">${inside}</svg>`;
    expect(pathOf(file('<path d="M0 0h8"/>'))).toBe("M0 0h8");
    for (const other of [
      file('<path d="M0 0h8"/><path d="M0 4h8"/>'),
      file('<circle r="4"/>'),
      file('<path d="M0 0h8"/>', "0 0 24 24"),
    ]) {
      expect(() => pathOf(other), other).toThrow("not a Material Symbols icon");
    }
  });

  it("can't be asked for by a name the registry doesn't have", () => {
    const known: IconName = "close";
    // @ts-expect-error: the type check refuses an icon that isn't in the registry.
    const unknown: IconName = "sparkles";
    // @ts-expect-error: and so does the component's own `name`.
    mount(Icon, { props: { name: "sparkles" } });
    expect(known in icons).toBe(true);
    expect(unknown in icons).toBe(false);
  });
});
