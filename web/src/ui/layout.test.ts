import { describe, expect, it } from "vitest";
import { h, nextTick } from "vue";

import Card from "@/ui/Card.vue";
import Collapsible from "@/ui/Collapsible.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Pagination from "@/ui/Pagination.vue";
import ScrollArea from "@/ui/ScrollArea.vue";
import Splitter from "@/ui/Splitter.vue";
import Tabs from "@/ui/Tabs.vue";
import { draw, drawModel } from "@/ui/test-kit";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";
import Tree from "@/ui/Tree.vue";
import { findBranch, type Branch } from "@/ui/tree";

describe("tabs", () => {
  const tabs = [
    { value: "log", label: "Log" },
    { value: "files", label: "Files" },
    { value: "off", label: "Off", disabled: true },
  ];

  it("are named, show the view chosen, and choose another by a press", async () => {
    const drawn = drawModel(
      Tabs,
      "log",
      { tabs, label: "Parts" },
      { log: () => "The log's view", files: () => "The files' view" },
    );
    const list = drawn.wrapper.get("[role='tablist']");
    expect(list.attributes("aria-label")).toBe("Parts");
    const each = drawn.wrapper.findAll("[role='tab']");
    expect(each.map((one) => one.text())).toEqual(["Log", "Files", "Off"]);
    expect(each[0]!.attributes("aria-selected")).toBe("true");
    expect(drawn.wrapper.get("[role='tabpanel']").text()).toBe("The log's view");

    await each[1]!.trigger("mousedown", { button: 0 });
    await nextTick();
    expect(drawn.changes).toEqual(["files"]);
    expect(drawn.wrapper.get("[role='tabpanel'][data-state='active']").text()).toBe(
      "The files' view",
    );
    expect(each[2]!.attributes("disabled")).toBeDefined();
  });
});

describe("a splitter", () => {
  it("has two panes and a handle that is named", () => {
    const { wrapper } = draw(
      Splitter,
      { label: "The log and the line" },
      { first: () => "first", second: () => "second" },
    );
    expect(wrapper.text()).toContain("first");
    expect(wrapper.text()).toContain("second");
    const handle = wrapper.get("[role='separator']");
    expect(handle.attributes("aria-label")).toBe("The log and the line");
    expect(handle.attributes("tabindex")).toBe("0");
  });
});

describe("a toolbar", () => {
  it("is one named row of buttons, and a button that stays pressed says so", async () => {
    let pressed = 0;
    const { wrapper } = draw(
      Toolbar,
      { label: "The log's tools" },
      {
        default: () => [
          h(ToolbarButton, { pressed: true, onClick: () => pressed++ }, () => "Errors only"),
          h(ToolbarButton, { icon: "down" }, () => "Newest first"),
          h(ToolbarButton, { disabled: true }, () => "Pick none"),
        ],
      },
    );
    const bar = wrapper.get("[role='toolbar']");
    expect(bar.attributes("aria-label")).toBe("The log's tools");
    const buttons = bar.findAll("button");
    expect(buttons.map((one) => one.text())).toEqual(["Errors only", "Newest first", "Pick none"]);
    expect(buttons[0]!.attributes("aria-pressed")).toBe("true");
    expect(buttons[0]!.attributes("data-state")).toBe("on");
    expect(buttons[1]!.attributes("aria-pressed")).toBeUndefined();
    expect(buttons[2]!.attributes("disabled")).toBeDefined();
    await buttons[0]!.trigger("click");
    expect(pressed).toBe(1);
  });
});

describe("a card and a section", () => {
  it("have a title at the page's level, three below the page's title when not told", () => {
    const card = draw(Card, { title: "Camera" }, { default: () => "detail" }).wrapper;
    const title = card.get("h3");
    expect(title.text()).toBe("Camera");
    expect(card.get("section").attributes("aria-labelledby")).toBe(title.attributes("id"));
    expect(draw(Card, { title: "Stage", level: 2 }).wrapper.find("h2").text()).toBe("Stage");

    const section = draw(Collapsible, { title: "Scan settings", level: 2 }).wrapper;
    expect(section.get("h2 button").text()).toBe("Scan settings");
  });

  it("opens and closes a section by its title", async () => {
    // Its model is `open`, not `modelValue`.
    const changes: boolean[] = [];
    const section = draw(
      Collapsible,
      {
        title: "Scan settings",
        open: false,
        "onUpdate:open": (now: boolean) => {
          changes.push(now);
          void section.set({ open: now });
        },
      },
      { default: () => "Each tile overlaps the next." },
    );
    const title = section.wrapper.get("button");
    expect(title.attributes("aria-expanded")).toBe("false");
    await title.trigger("click");
    await nextTick();
    expect(changes).toEqual([true]);
    expect(title.attributes("aria-expanded")).toBe("true");
    expect(section.wrapper.text()).toContain("Each tile overlaps the next.");
  });
});

describe("pages", () => {
  it("say which is shown, and go to another by its button", async () => {
    const pages = drawModel(Pagination, 1, { total: 240, perPage: 20 });
    const nav = pages.wrapper.get("nav");
    expect(nav.attributes("aria-label")).toBe("Pages");
    const page = (name: string) =>
      nav.findAll("button").find((one) => one.attributes("aria-label") === name)!;
    expect(page("Page 1").attributes("aria-current")).toBe("page");
    expect(page("Page 12").exists()).toBe(true);
    expect(page("The page before").attributes("disabled")).toBeDefined();
    await page("Page 2").trigger("click");
    expect(pages.changes).toEqual([2]);
    expect(page("Page 2").attributes("aria-current")).toBe("page");
    expect(page("Page 1").attributes("aria-current")).toBeUndefined();
  });
});

describe("a part that scrolls on its own, and terms with their values", () => {
  it("is a named region the keyboard reaches", () => {
    const area = draw(ScrollArea, { label: "Release notes" }, { default: () => "notes" }).wrapper;
    const region = area.get("[role='region']");
    expect(region.attributes("aria-label")).toBe("Release notes");
    expect(region.attributes("tabindex")).toBe("0");
  });

  it("lists each term with its value", () => {
    const entries = [
      { term: "Model", value: "MIChrome 20" },
      { term: "Pixels", value: "5440 × 3648" },
    ];
    const list = draw(KeyValue, { entries }).wrapper.get("dl");
    expect(list.findAll("dt").map((one) => one.text())).toEqual(["Model", "Pixels"]);
    expect(list.findAll("dd").map((one) => one.text())).toEqual(["MIChrome 20", "5440 × 3648"]);
  });
});

describe("a tree", () => {
  const items: Branch[] = [
    {
      id: "scans",
      label: "scans",
      children: [
        {
          id: "scans/slide-4",
          label: "slide-4",
          children: [{ id: "scans/slide-4/a", label: "a.tiff" }],
        },
        { id: "scans/slide-5", label: "slide-5" },
      ],
    },
    { id: "captures", label: "captures" },
    { id: "calibration", label: "calibration" },
  ];
  const tree = (expanded: string[] = ["scans"]) => {
    const drawn = drawModel<string | undefined>(Tree, undefined, {
      items,
      label: "Files",
      expanded,
    });
    const item = (name: string) =>
      drawn.wrapper.findAll("[role='treeitem']").find((one) => one.text() === name)!;
    return { ...drawn, item };
  };
  const type = async (key: string) => {
    document.activeElement!.dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await nextTick();
  };

  it("finds an item anywhere by its id", () => {
    expect(findBranch(items, "scans/slide-4/a")?.label).toBe("a.tiff");
    expect(findBranch(items, "nowhere")).toBeUndefined();
    expect(findBranch(items, undefined)).toBeUndefined();
  });

  it("is named, shows the items open, and picks one by a press", async () => {
    const files = tree();
    expect(files.wrapper.get("[role='tree']").attributes("aria-label")).toBe("Files");
    expect(files.wrapper.findAll("[role='treeitem']").map((one) => one.text())).toEqual([
      "scans",
      "slide-4",
      "slide-5",
      "captures",
      "calibration",
    ]);
    expect(files.item("scans").attributes("aria-expanded")).toBe("true");
    expect(files.item("scans").attributes("aria-level")).toBe("1");
    expect(files.item("slide-4").attributes("aria-level")).toBe("2");
    await files.item("captures").trigger("click");
    expect(files.changes).toEqual(["captures"]);
  });

  it("goes to an item by its first letters, after any other key", async () => {
    const files = tree();
    (files.item("scans").element as HTMLElement).focus();
    // A letter first, and again after another key.
    await type("c");
    expect(document.activeElement).toBe(files.item("captures").element);
    await type("Home");
    await type("c");
    expect(document.activeElement).toBe(files.item("captures").element);
    // The same letter again goes on to the next.
    await type("c");
    expect(document.activeElement).toBe(files.item("calibration").element);
    // More letters stay where they still fit.
    await type("End");
    (files.item("scans").element as HTMLElement).focus();
    await type("c");
    await type("a");
    await type("l");
    expect(document.activeElement).toBe(files.item("calibration").element);
    // Nothing found, nothing moves: Home went to the first, and there it stays.
    await type("Home");
    await type("z");
    expect(document.activeElement).toBe(files.item("scans").element);
  });
});
