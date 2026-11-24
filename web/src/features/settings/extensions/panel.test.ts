// The extensions' panels, from the sample extensions' own descriptions.
import { describe, expect, it } from "vitest";

// The sample extensions' panels, as their packages hold them.
import greeting from "../../../../../examples/ex-omext/hello/panels/hello.greeting.json";
import denoise from "../../../../../examples/ext-python-node/panels/example.denoise_tv.json";
import emboss from "../../../../../examples/ext-rust-node/panels/example.emboss.json";
import { readPanel, shownOf } from "@/features/settings/extensions/panel";
import { shapeOf } from "@/ui/schema-form/shape";

const SAMPLES = { emboss, denoise, greeting };

describe("a panel's description", () => {
  it("is drawn for each sample extension: its form, its node, its view", () => {
    const emboss = readPanel(SAMPLES.emboss);
    expect(emboss.unknown).toEqual([]);
    expect(emboss.panel).toMatchObject({
      title: "Emboss the live view",
      submit: { kind: "node", node: "example.emboss" },
      view: "image",
    });
    const fields = (form: unknown) => {
      const shape = shapeOf(form);
      return shape.kind === "object" ? shape.fields.map((f) => [f.name, f.shape.kind]) : [];
    };
    expect(fields(emboss.panel!.form)).toEqual([["strength", "number"]]);
    const denoise = readPanel(SAMPLES.denoise);
    expect([denoise.unknown, denoise.panel?.submit]).toEqual([
      [],
      { kind: "node", node: "example.denoise_tv" },
    ]);
    expect(fields(denoise.panel!.form)).toEqual([
      ["weight", "number"],
      ["iterations", "number"],
    ]);
    const greeting = readPanel(SAMPLES.greeting);
    expect([greeting.unknown, fields(greeting.panel!.form)]).toEqual([[], [["name", "string"]]]);
  });

  it("submitting to a Thing's action is drawn too", () => {
    const action = { kind: "action", thing: "stage", action: "move_to" };
    const read = readPanel({ ...(SAMPLES.emboss as object), submit: action, view: "json" });
    expect([read.unknown, read.panel?.submit, read.panel?.view]).toEqual([[], action, "json"]);
  });

  it("says what the app doesn't know, and draws none it can't", () => {
    const base = SAMPLES.emboss as Record<string, unknown>;
    // A view, and a submission, of kinds it doesn't know: no panel.
    const chart = readPanel({ ...base, view: "chart" });
    expect([chart.panel, chart.unknown]).toEqual([
      undefined,
      ["its view, `chart`, isn't an image, a table, JSON or text"],
    ]);
    const script = readPanel({ ...base, submit: { kind: "script", code: "…" } });
    expect(script.unknown).toEqual([
      "its submission, of the kind `script`, isn't a node's or an action's",
    ]);
    expect(readPanel("a panel").unknown).toEqual(["it isn't a panel's description"]);
    // A field the form can only show as JSON: drawn, and said.
    const form = {
      type: "object",
      properties: { strength: { type: "number" }, names: { type: "string", pattern: "^a" } },
    };
    const field = readPanel({ ...base, form });
    expect(field.panel).toBeDefined();
    expect(field.unknown).toEqual(["names: it uses `pattern`"]);
  });
});

describe("a submission's answer", () => {
  it("is shown as its view says: the node's image, or its table", () => {
    const answer = {
      histogram: { table: { columns: ["bin", "count"], rows: [[0, 5]] } },
      image: { image: { width: 4, height: 2, png: "iVBO" } },
    };
    expect(shownOf("image", answer)).toEqual({
      kind: "image",
      src: "data:image/png;base64,iVBO",
      width: 4,
      height: 2,
      port: "image",
    });
    expect(shownOf("table", answer)).toEqual({
      kind: "table",
      columns: ["bin", "count"],
      rows: [[0, 5]],
      port: "histogram",
    });
    expect(shownOf("image", { count: 3 })).toEqual({ kind: "nothing" });
  });

  it("is shown as text or JSON otherwise: an action's output as it is", () => {
    expect(shownOf("text", { greeting: "hello", to: "world" })).toEqual({
      kind: "text",
      text: "hello\nworld",
    });
    expect(shownOf("json", { x: 1 })).toEqual({ kind: "text", text: '{\n  "x": 1\n}' });
    expect(shownOf("text", "moved")).toEqual({ kind: "text", text: "moved" });
    expect(shownOf("json", undefined)).toEqual({ kind: "nothing" });
  });
});
