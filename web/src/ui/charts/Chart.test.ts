import { describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";

import Chart from "@/ui/charts/Chart.vue";
import type { Series } from "@/ui/charts/draw";
import { draw } from "@/ui/test-kit";

const profile: Series = {
  name: "Mean",
  kind: "line",
  x: [0, 25, 50, 75, 100],
  y: [10, 20, 30, 20, 10],
};

/** A chart, its plot, and keys pressed on it. */
function chart(props: Record<string, unknown> = {}) {
  const drawn = draw(Chart, {
    title: "A profile",
    series: [profile],
    x: { title: "Position (µm)" },
    y: { title: "Value (ADU)" },
    ...props,
  });
  const plot = drawn.wrapper.get("[role=img]");
  return {
    ...drawn,
    plot,
    readout: () => drawn.wrapper.get("output").text(),
    showAll: () => drawn.wrapper.findAll("button").find((one) => one.text() === "Show all"),
    async key(name: string): Promise<void> {
      await plot.trigger("keydown", { key: name });
      await nextTick();
    },
  };
}

describe("a chart", () => {
  it("is an image named by its title and what it shows, and one stop for Tab", () => {
    const { plot } = chart();
    expect(plot.attributes("tabindex")).toBe("0");
    expect(plot.attributes("aria-label")).toBe(
      "A profile. It shows a line of Mean, along Position (µm) from 0.00 to 100.00, and Value (ADU) from 10.00 to 30.00. The numbers are in the table below it.",
    );
    expect(
      chart({ series: [{ ...profile, kind: "histogram", width: 25 }] }).plot.attributes(
        "aria-label",
      ),
    ).toContain(
      "It shows a histogram of Mean, along Position (µm) from 0.00 to 125.00, and Value (ADU) from 0.00 to 40.00.",
    );
  });

  it("reads the value under its cursor, moved from point to point by the keyboard", async () => {
    const drawn = chart();
    expect(drawn.readout()).toBe("");
    expect(drawn.wrapper.get("output").attributes("aria-live")).toBe("polite");
    await drawn.key("ArrowRight");
    expect(drawn.readout()).toBe("Position (µm) 50.0; Mean 30.0");
    await drawn.key("ArrowRight");
    expect(drawn.readout()).toBe("Position (µm) 75.0; Mean 20.0");
    await drawn.key("End");
    await drawn.key("ArrowRight");
    expect(drawn.readout()).toBe("Position (µm) 100.0; Mean 10.0");
    await drawn.key("Home");
    expect(drawn.readout()).toBe("Position (µm) 0.0; Mean 10.0");
  });

  it("reads each series at the cursor, and says where one is off a log scale", async () => {
    const drawn = chart({
      series: [profile, { name: "Floor", kind: "line", x: profile.x, y: [0, 1, 10, 100, 1000] }],
      y: { title: "Value (ADU)", scale: "log" },
    });
    await drawn.key("Home");
    expect(drawn.readout()).toBe("Position (µm) 0.0; Mean 10.0; Floor not on this scale");
    await drawn.key("End");
    expect(drawn.readout()).toBe("Position (µm) 100.0; Mean 10.0; Floor 1000");
  });

  it("reads a histogram's bin, from its lower edge to its upper", async () => {
    const drawn = chart({
      series: [{ name: "Pixels", kind: "histogram", x: [0, 16, 32], y: [5, 9, 2], width: 16 }],
    });
    await drawn.key("ArrowRight");
    expect(drawn.readout()).toBe("Position (µm) from 16.0 to 32.0; Pixels 9.0");
  });

  it("zooms in and out from the keyboard, and shows everything again", async () => {
    const drawn = chart();
    expect(drawn.showAll()).toBeUndefined();
    await drawn.key("+");
    expect(drawn.showAll()).toBeDefined();
    // Zoomed out as far as the data goes is everything again.
    await drawn.key("-");
    expect(drawn.showAll()).toBeUndefined();
    await drawn.key("+");
    await drawn.key("PageDown");
    await drawn.key("0");
    expect(drawn.showAll()).toBeUndefined();
    await drawn.key("+");
    await drawn.showAll()!.trigger("click");
    expect(drawn.showAll()).toBeUndefined();
  });

  it("fits its y range to the values in view", async () => {
    const peak: Series = {
      name: "Mean",
      kind: "line",
      x: [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
      y: [10, 10, 12, 14, 30, 50, 30, 14, 12, 10, 10],
    };
    const drawn = chart({ series: [peak] });
    expect(drawn.plot.attributes("aria-label")).toContain("Value (ADU) from 0.00 to 60.00");
    await drawn.key("Home");
    await drawn.key("+");
    await drawn.key("+");
    expect(drawn.plot.attributes("aria-label")).toContain("Value (ADU) from 10.00 to 12.00");
  });

  it("reads its colours again when the page's theme or accent changes", async () => {
    chart();
    const read = vi.spyOn(window, "getComputedStyle");
    document.documentElement.dataset.theme = "dark";
    await vi.waitFor(() => expect(read).toHaveBeenCalled());
    read.mockClear();
    document.documentElement.dataset.accent = "teal";
    await vi.waitFor(() => expect(read).toHaveBeenCalled());
    read.mockRestore();
    delete document.documentElement.dataset.theme;
    delete document.documentElement.dataset.accent;
  });

  it("names its series in a legend, each with its colour and its dash or mark", () => {
    const { wrapper } = chart({
      series: [profile, { ...profile, name: "Fit", kind: "points" }, { ...profile, name: "Floor" }],
    });
    const items = wrapper.findAll(".legend li");
    expect(items.map((one) => one.text())).toEqual(["Mean", "Fit", "Floor"]);
    expect(items[0]!.get("line").attributes("style")).toContain("stroke: var(--chart-1)");
    expect(items[1]!.find("rect").exists()).toBe(true);
    expect(items[2]!.get("line").attributes("style")).toContain("stroke-dasharray: 2 3");
    expect(chart().wrapper.find(".legend").exists()).toBe(false);
  });

  it("holds the same numbers in a table behind it", async () => {
    const { wrapper } = chart({
      bands: [{ of: 0, x: profile.x, lower: [9, 18, 27, 18, 9], upper: [11, 22, 33, 22, 11] }],
    });
    const section = wrapper.get("button[aria-expanded]");
    expect(section.text()).toBe("The numbers");
    await section.trigger("click");
    await nextTick();
    const grid = wrapper.get("[role=grid]");
    expect(grid.attributes("aria-label")).toBe("The numbers of A profile");
    expect(grid.attributes("aria-rowcount")).toBe("6");
    expect(grid.findAll("[role=columnheader]").map((one) => one.text())).toEqual([
      "Position (µm)",
      "Mean",
      "Mean, low",
      "Mean, high",
    ]);
  });

  it("lists each point by its series, where the series' x differ", async () => {
    const { wrapper } = chart({
      series: [profile, { name: "Fine", kind: "points", x: [40, 60], y: [28, 29] }],
    });
    await wrapper.get("button[aria-expanded]").trigger("click");
    await nextTick();
    const grid = wrapper.get("[role=grid]");
    expect(grid.findAll("[role=columnheader]").map((one) => one.text())).toEqual([
      "Series",
      "Position (µm)",
      "Value (ADU)",
    ]);
    expect(grid.attributes("aria-rowcount")).toBe("8");
  });
});
