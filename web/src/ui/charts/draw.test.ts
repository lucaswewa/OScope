import { describe, expect, it } from "vitest";

import {
  DASHES,
  drawAxes,
  drawBand,
  drawCursor,
  drawSeries,
  type Colours,
  type Series,
} from "@/ui/charts/draw";
import type { Scale } from "@/ui/charts/scale";

/** A canvas's 2D context that writes down what it is told, and draws nothing. */
function recorder() {
  const calls: string[] = [];
  const state: Record<string, unknown> = { lineDash: [] };
  const handler: ProxyHandler<Record<string, unknown>> = {
    get(_, name: string) {
      if (name === "calls") return calls;
      if (name === "measureText") return (text: string) => ({ width: text.length * 7 });
      if (name === "setLineDash")
        return (dash: number[]) => calls.push(`setLineDash ${dash.join(",")}`);
      if (name in state) return state[name];
      return (...args: unknown[]) => {
        calls.push(
          `${name} ${args.map((arg) => (typeof arg === "number" ? Math.round(arg * 10) / 10 : String(arg))).join(",")}`,
        );
      };
    },
    set(_, name: string, value: unknown) {
      state[name] = value;
      calls.push(`${name}=${String(value)}`);
      return true;
    },
  };
  const pen = new Proxy({}, handler) as unknown as CanvasRenderingContext2D & { calls: string[] };
  return { pen, calls };
}

const colours: Colours = {
  series: ["s1", "s2", "s3", "s4", "s5", "s6"],
  grid: "grid",
  axis: "axis",
  text: "text",
  font: "13px sans-serif",
};
const x: Scale = { kind: "linear", min: 0, max: 100, from: 0, to: 400 };
const y: Scale = { kind: "linear", min: 0, max: 10, from: 200, to: 0 };
const line = (n: number, kind: Series["kind"] = "line"): Series => ({
  name: `series ${n}`,
  kind,
  x: [0, 50, 100],
  y: [0, 5, 10],
});

describe("a chart drawn on a canvas", () => {
  it("draws each series in its own colour and dash, so that colour isn't all that tells them apart", () => {
    for (let n = 0; n < 4; n++) {
      const { pen, calls } = recorder();
      drawSeries(pen, line(n), n, x, y, colours);
      expect(calls).toContain(`strokeStyle=s${n + 1}`);
      expect(calls).toContain(`setLineDash ${DASHES[n]!.join(",")}`);
      expect(calls.filter((call) => call.startsWith("lineTo"))).toEqual([
        "lineTo 200,100",
        "lineTo 400,0",
      ]);
    }
  });

  it("draws points as marks, each series its own shape", () => {
    const shapes = [0, 1, 2, 3].map((n) => {
      const { pen, calls } = recorder();
      drawSeries(pen, line(n, "points"), n, x, y, colours);
      return calls.filter((call) => /^(arc|rect|closePath)/.test(call)).length === 0
        ? "none"
        : calls.some((call) => call.startsWith("arc"))
          ? "circle"
          : calls.some((call) => call.startsWith("rect"))
            ? "square"
            : calls.filter((call) => call.startsWith("lineTo")).length === 6
              ? "triangle"
              : "diamond";
    });
    expect(shapes).toEqual(["circle", "square", "triangle", "diamond"]);
  });

  it("fills an area down to the axis, and leaves out values a log scale can't show", () => {
    const { pen, calls } = recorder();
    drawSeries(pen, line(0, "area"), 0, x, y, colours);
    expect(calls).toContain("globalAlpha=0.25");
    expect(calls.filter((call) => call.startsWith("lineTo")).slice(-2)).toEqual([
      "lineTo 400,200",
      "lineTo 0,200",
    ]);
    const log: Scale = { kind: "log", min: 1, max: 100, from: 200, to: 0 };
    const gap = recorder();
    drawSeries(
      gap.pen,
      { name: "", kind: "line", x: [0, 25, 50, 75], y: [10, 0, 10, 100] },
      0,
      x,
      log,
      colours,
    );
    expect(gap.calls.filter((call) => /^(moveTo|lineTo)/.test(call))).toEqual([
      "moveTo 0,100",
      "moveTo 200,100",
      "lineTo 300,0",
    ]);
  });

  it("draws a histogram as steps, a column for each bin wider than a pixel", () => {
    const { pen, calls } = recorder();
    const bins: Series = {
      name: "",
      kind: "histogram",
      x: [0, 25, 50, 75],
      y: [2, 4, 6, 8],
      width: 25,
    };
    drawSeries(pen, bins, 0, x, y, colours);
    expect(calls.filter((call) => /^(moveTo|lineTo)/.test(call))).toEqual([
      "moveTo 0,200",
      "lineTo 0,160",
      "lineTo 100,160",
      "lineTo 100,120",
      "lineTo 200,120",
      "lineTo 200,80",
      "lineTo 300,80",
      "lineTo 300,40",
      "lineTo 400,40",
      "lineTo 400,200",
    ]);
    expect(calls).toContain("globalAlpha=0.3");
  });

  it("draws 4,096 bins in 400 columns, each the highest of its bins, so that no peak is lost", () => {
    const { pen, calls } = recorder();
    const counts = Array.from({ length: 4096 }, (_, n) => (n === 1234 ? 10 : 1));
    const histogram: Series = {
      name: "",
      kind: "histogram",
      x: Array.from(counts, (_, n) => n * 16),
      y: counts,
      width: 16,
    };
    const wide: Scale = { kind: "linear", min: 0, max: 65536, from: 0, to: 400 };
    drawSeries(pen, histogram, 0, wide, y, colours);
    const steps = calls.filter((call) => call.startsWith("lineTo"));
    expect(steps.length).toBeLessThanOrEqual(2 * 400 + 2);
    // The one tall bin is drawn at its full height, in its column.
    const peak = Math.floor((1234 * 16 * 400) / 65536);
    expect(steps).toContain(`lineTo ${peak},0`);
    expect(steps).toContain(`lineTo ${peak + 1},0`);
  });

  it("draws only the bins in view", () => {
    const { pen, calls } = recorder();
    const bins: Series = {
      name: "",
      kind: "histogram",
      x: [0, 25, 50, 75],
      y: [2, 4, 6, 8],
      width: 25,
    };
    drawSeries(pen, bins, 0, { ...x, min: 50, max: 100 }, y, colours);
    expect(calls.filter((call) => /^(moveTo|lineTo)/.test(call))[0]).toBe("moveTo 0,200");
    expect(calls.filter((call) => call.startsWith("lineTo")).length).toBe(5);
  });

  it("fills a band faintly between its lines, in its series' colour", () => {
    const { pen, calls } = recorder();
    drawBand(pen, { of: 1, x: [0, 100], lower: [1, 2], upper: [3, 4] }, x, y, colours);
    expect(calls).toContain("fillStyle=s2");
    expect(calls).toContain("globalAlpha=0.2");
    expect(calls.filter((call) => call.startsWith("lineTo"))).toEqual([
      "lineTo 0,140",
      "lineTo 400,120",
      "lineTo 400,160",
      "lineTo 0,180",
    ]);
  });

  it("draws the grid at the ticks, and writes the ticks and the axes' titles", () => {
    const { pen, calls } = recorder();
    const frame = { left: 40, top: 8, right: 400, bottom: 200, height: 240 };
    drawAxes(
      pen,
      frame,
      {
        scale: { ...x, from: 40 },
        ticks: [
          { value: 0, label: "0" },
          { value: 100, label: "100" },
        ],
        title: "Value (ADU)",
      },
      { scale: y, ticks: [{ value: 5, label: "5" }], title: "Pixels" },
      colours,
    );
    expect(calls).toContain("strokeStyle=grid");
    expect(calls).toContain("strokeStyle=axis");
    expect(calls.filter((call) => call.startsWith("fillText"))).toEqual([
      "fillText 0,40,206",
      "fillText 100,400,206",
      "fillText 5,34,100",
      "fillText Value (ADU),220,240",
      "fillText Pixels,0,0",
    ]);
  });

  it("draws the cursor's line, and a ring on each series' point, but none off the scale", () => {
    const { pen, calls } = recorder();
    const frame = { left: 0, top: 0, right: 400, bottom: 200, height: 240 };
    drawCursor(
      pen,
      frame,
      200,
      [
        { x: 200, y: 100, n: 0 },
        { x: 200, y: Number.NaN, n: 1 },
      ],
      colours,
    );
    expect(calls).toContain("moveTo 200.5,0");
    expect(calls.filter((call) => call.startsWith("arc"))).toHaveLength(1);
    expect(calls).toContain("strokeStyle=s1");
  });
});
