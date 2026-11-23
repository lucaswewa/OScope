import { describe, expect, it } from "vitest";
import { h } from "vue";

import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import EmptyState from "@/ui/EmptyState.vue";
import Field from "@/ui/Field.vue";
import Progress from "@/ui/Progress.vue";
import Skeleton from "@/ui/Skeleton.vue";
import Tag from "@/ui/Tag.vue";
import { draw } from "@/ui/test-kit";

describe("a banner", () => {
  const banner = (props: Record<string, unknown> = {}, slots: Record<string, () => unknown> = {}) =>
    draw(Banner, props, { default: () => "The stage hasn't been homed.", ...slots }).wrapper;

  it("tells of something, quietly, unless told its level", () => {
    const told = banner().get(".banner");
    expect(told.classes()).toContain("info");
    expect(told.attributes("role")).toBe("status");
    expect(told.get(".message").text()).toBe("The stage hasn't been homed.");
    expect(told.find("button").exists()).toBe(false);
    expect(banner({ level: "success" }).get(".banner").attributes("role")).toBe("status");
  });

  it("is announced at once when it warns, or says something failed", () => {
    for (const level of ["warning", "danger"]) {
      const urgent = banner({ level }).get(".banner");
      expect(urgent.attributes("role"), level).toBe("alert");
      expect(urgent.classes(), level).toContain(level);
    }
  });

  it("has a few stronger words before its message, and what to do about it after", () => {
    const full = banner({ title: "Not homed." }, { actions: () => h("button", "Home it") });
    expect(full.get(".message strong").text()).toBe("Not homed.");
    expect(full.get(".actions button").text()).toBe("Home it");
    expect(banner().find(".actions").exists()).toBe(false);
  });

  it("can be sent away only where its page lets it, and says so when it is", async () => {
    let dismissed = 0;
    const closable = banner({ dismissable: true, onDismiss: () => dismissed++ });
    const close = closable.get("button");
    expect(close.attributes("aria-label")).toBe("Dismiss");
    await close.trigger("click");
    expect(dismissed).toBe(1);
  });
});

describe("progress", () => {
  const bar = (props: Record<string, unknown>) =>
    draw(Progress, { label: "Scanning", ...props }).wrapper.get("[role='progressbar']");

  it("says how far, of how much", () => {
    const scanning = bar({ value: 42 });
    expect(scanning.attributes()).toMatchObject({
      "aria-label": "Scanning",
      "aria-valuemin": "0",
      "aria-valuemax": "100",
      "aria-valuenow": "42",
    });
    expect(scanning.get(".fill").attributes("style")).toContain("42%");
    expect(scanning.classes()).not.toContain("unknown");

    const tiles = bar({ value: 30, max: 120 });
    expect(tiles.attributes("aria-valuemax")).toBe("120");
    expect(tiles.get(".fill").attributes("style")).toContain("25%");
  });

  it("says nothing of how far where no end is known", () => {
    for (const props of [{}, { value: null }]) {
      const looking = bar(props);
      expect(looking.attributes("aria-valuenow")).toBeUndefined();
      expect(looking.classes()).toContain("unknown");
    }
  });

  it("draws no more than all of it, and no less than none", () => {
    expect(bar({ value: 150 }).get(".fill").attributes("style")).toContain("100%");
    expect(bar({ value: -5 }).get(".fill").attributes("style")).toContain("0%");
    // Nothing to do at all isn't a share of anything.
    expect(bar({ value: 5, max: 0 }).classes()).toContain("unknown");
  });

  it("is a ring where asked, with the same to say", () => {
    const ring = bar({ value: 50, ring: true });
    expect(ring.classes()).toContain("ring");
    expect(ring.attributes("aria-valuenow")).toBe("50");
    const [dash, whole] = ring.get(".arc").attributes("stroke-dasharray")!.split(" ").map(Number);
    expect(dash! / whole!).toBeCloseTo(0.5, 6);
    expect(ring.get("svg").attributes("aria-hidden")).toBe("true");
    expect(ring.find(".fill").exists()).toBe(false);
  });

  it("is named by the field around it", () => {
    const { wrapper } = draw(
      Field,
      { label: "Scanning" },
      { default: () => h(Progress, { value: 1 }) },
    );
    expect(wrapper.get("[role='progressbar']").attributes("aria-labelledby")).toBe(
      wrapper.get("label").attributes("id"),
    );
  });
});

describe("a skeleton", () => {
  it("has the size it stands in for, and says nothing", () => {
    const block = draw(Skeleton, { width: "60%", height: "6rem" }).wrapper.get(".skeleton");
    expect(block.attributes("aria-hidden")).toBe("true");
    expect(block.attributes("style")).toContain("inline-size: 60%");
    expect(block.attributes("style")).toContain("block-size: 6rem");
    const line = draw(Skeleton).wrapper.get(".skeleton").attributes("style");
    expect(line).toContain("inline-size: 100%");
    expect(line).toContain("block-size: 1em");
  });
});

describe("an empty state", () => {
  it("says what is missing, why, and what to do", () => {
    const { wrapper } = draw(
      EmptyState,
      { title: "Nothing has been saved yet", icon: "folder" },
      { default: () => "Captures appear here.", actions: () => h("button", "Capture") },
    );
    expect(wrapper.get(".title").text()).toBe("Nothing has been saved yet");
    expect(wrapper.get(".more").text()).toBe("Captures appear here.");
    expect(wrapper.get(".actions button").text()).toBe("Capture");
    expect(wrapper.get("svg").attributes("aria-hidden")).toBe("true");

    const bare = draw(EmptyState, { title: "No logs" }).wrapper;
    expect(bare.find("svg").exists()).toBe(false);
    expect(bare.find(".more").exists()).toBe(false);
    expect(bare.find(".actions").exists()).toBe(false);
  });
});

describe("a badge and a tag", () => {
  it("has its word, in its tone", () => {
    const failed = draw(Badge, { tone: "danger" }, { default: () => "Failed" }).wrapper.get(
      ".badge",
    );
    expect(failed.text()).toBe("Failed");
    expect(failed.classes()).toContain("danger");
    expect(
      draw(Badge, {}, { default: () => "12" })
        .wrapper.get(".badge")
        .classes(),
    ).toContain("neutral");
  });

  it("is taken off by its own button, named for it", async () => {
    let removed = 0;
    const tag = draw(Tag, { label: "H&E", removable: true, onRemove: () => removed++ }).wrapper;
    expect(tag.get(".tag").text()).toBe("H&E");
    const button = tag.get("button");
    expect(button.attributes("aria-label")).toBe("Remove H&E");
    expect(button.attributes("type")).toBe("button");
    await button.trigger("click");
    expect(removed).toBe(1);
    expect(draw(Tag, { label: "simulated" }).wrapper.find("button").exists()).toBe(false);
  });
});
