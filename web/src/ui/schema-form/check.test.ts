import { describe, expect, it } from "vitest";

import {
  faultsOf,
  keyOf,
  placed,
  placesOf,
  refusals,
  same,
  valueAt,
  type Fault,
} from "@/ui/schema-form/check";
import { shapeOf, type Json } from "@/ui/schema-form/shape";

/** The faults of a value, as `path why`, with what each says. */
function faults(schema: unknown, value: Json | undefined): string[] {
  return faultsOf(shapeOf(schema), value).map((fault) =>
    `${fault.path.join("/")} ${fault.why}${fault.values ? ` ${JSON.stringify(fault.values)}` : ""}`.trim(),
  );
}

const scan = {
  type: "object",
  properties: {
    name: { type: "string", minLength: 1 },
    tiles: { type: "integer", minimum: 1, maximum: 400 },
    note: { type: ["string", "null"] },
  },
  required: ["name", "tiles"],
  additionalProperties: false,
};

describe("a value checked against its schema", () => {
  it("has no fault when it is of its shape", () => {
    expect(faults(scan, { name: "slide 4", tiles: 16 })).toEqual([]);
    expect(faults(scan, { name: "slide 4", tiles: 16, note: null })).toEqual([]);
    expect(faults({}, { anything: [1, "two"] })).toEqual([]);
    expect(faults({ type: "string", pattern: "x" }, 5)).toEqual([]);
  });

  it("says what must be given, and what isn't one of an object's fields", () => {
    expect(faults(scan, { name: "", note: "x", extra: 1 })).toEqual([
      "name needed",
      "tiles needed",
      "extra unknown",
    ]);
    expect(faults(scan, [1])).toEqual(["object"]);
    expect(faults(scan, undefined)).toEqual(["needed"]);
    // Where others may be added, they are left alone.
    expect(faults({ type: "object", properties: {} }, { more: 1 })).toEqual([]);
  });

  it("checks numbers against their kind and range", () => {
    const number = { type: "integer", minimum: 1, maximum: 400 };
    expect(faults(number, "16")).toEqual(["number"]);
    expect(faults(number, Number.NaN)).toEqual(["number"]);
    expect(faults(number, 1.5)).toEqual(["whole"]);
    expect(faults(number, 0)).toEqual(['least {"least":1}']);
    expect(faults(number, 401)).toEqual(['most {"most":400}']);
    const open = { type: "number", exclusiveMinimum: 0, exclusiveMaximum: 1 };
    expect(faults(open, 0)).toEqual(['above {"above":0}']);
    expect(faults(open, 1)).toEqual(['below {"below":1}']);
    expect(faults(open, 0.5)).toEqual([]);
  });

  it("checks strings' lengths, by the characters in them, and their formats", () => {
    expect(faults({ type: "string" }, 3)).toEqual(["text"]);
    expect(faults({ type: "string", minLength: 1 }, "")).toEqual(["needed"]);
    expect(faults({ type: "string", minLength: 3 }, "ab")).toEqual(['shortest {"count":3}']);
    // 𝜇 is two of JavaScript's units, and one character.
    expect(faults({ type: "string", maxLength: 2 }, "𝜇𝜇")).toEqual([]);
    expect(faults({ type: "string", maxLength: 2 }, "abc")).toEqual(['longest {"count":2}']);
    const format = (name: string, value: string): string[] =>
      faults({ type: "string", format: name }, value);
    expect(format("uuid", "3F2C1A9E-8B7D-4C6E-9A1F-2B3C4D5E6F70")).toEqual([]);
    expect(format("uuid", "3f2c1a9e")).toEqual(["uuid"]);
    expect(format("date-time", "2026-11-18T09:30:00Z")).toEqual([]);
    expect(format("date-time", "2026-11-18T09:30:00.125+01:00")).toEqual([]);
    expect(format("date-time", "2026-11-18")).toEqual(["dateTime"]);
    expect(format("date", "2026-11-18")).toEqual([]);
    expect(format("date", "18/11/2026")).toEqual(["date"]);
    expect(format("ip", "192.168.1.20")).toEqual([]);
    expect(format("ip", "::1")).toEqual([]);
    expect(format("ip", "fe80::1ff:fe23:4567:890a")).toEqual([]);
    expect(format("ip", "256.1.1.1")).toEqual(["ip"]);
    expect(format("ip", "host")).toEqual(["ip"]);
  });

  it("checks choices, yes or no, and constants", () => {
    expect(faults({ enum: ["4x", "10x"] }, "20x")).toEqual(["choice"]);
    expect(faults({ enum: [{ a: 1 }] }, { a: 1 })).toEqual([]);
    expect(faults({ type: "boolean" }, "yes")).toEqual(["boolean"]);
    expect(faults({ const: "v1" }, "v2")).toEqual(['constant {"value":"\\"v1\\""}']);
  });

  it("checks a list's length, and each item by its place", () => {
    const list = { type: "array", items: { type: "number", minimum: 0 }, minItems: 1, maxItems: 2 };
    expect(faults(list, {})).toEqual(["list"]);
    expect(faults(list, [])).toEqual(['fewest {"count":1}']);
    expect(faults(list, [1, 2, 3])).toEqual(['mostItems {"count":2}']);
    expect(faults(list, [1, -1])).toEqual(['1 least {"least":0}']);
  });

  it("checks a map's values by their names, and a value that may be left out only when it is given", () => {
    const map = { type: "object", additionalProperties: { type: "number" } };
    expect(faults(map, { red: 1, blue: "x" })).toEqual(["blue number"]);
    expect(faults(map, 3)).toEqual(["object"]);
    expect(faults({ type: ["number", "null"], minimum: 1 }, null)).toEqual([]);
    expect(faults({ type: ["number", "null"], minimum: 1 }, 0)).toEqual(['least {"least":1}']);
  });

  it("checks a union's kind, and what that kind holds, where it is drawn", () => {
    const tagged = {
      oneOf: [
        {
          type: "object",
          properties: { kind: { const: "spiral" }, turns: { type: "integer", minimum: 1 } },
          required: ["kind", "turns"],
        },
        { type: "object", properties: { kind: { const: "snake" } }, required: ["kind"] },
      ],
    };
    expect(faults(tagged, { kind: "spiral", turns: 0 })).toEqual(['turns least {"least":1}']);
    expect(faults(tagged, { kind: "zigzag" })).toEqual(["kind variant"]);
    const keyed = {
      oneOf: [
        { const: "whole" },
        {
          type: "object",
          properties: {
            rect: { type: "object", properties: { w: { type: "number", minimum: 0 } } },
          },
          required: ["rect"],
        },
      ],
    };
    expect(faults(keyed, "whole")).toEqual([]);
    expect(faults(keyed, { rect: { w: -1 } })).toEqual(['rect/w least {"least":0}']);
    expect(faults(keyed, "part")).toEqual(["variant"]);
  });
});

describe("the server's refusals", () => {
  it("are the faults a 422 lists, each where its place says, from the request's body", () => {
    const answer = {
      detail: [
        {
          type: "uuid_parsing",
          loc: ["body", "id"],
          msg: "Input should be a valid UUID",
          input: "",
        },
        {
          type: "greater_than_equal",
          loc: ["body", "corners", 1, "x"],
          msg: "Input should be at least 0",
          input: -1,
        },
        { type: "missing", loc: ["body"], msg: "Field required", input: null },
        { loc: ["body", "x"] },
        "junk",
      ],
    };
    expect(refusals(answer)).toEqual([
      { path: ["id"], why: "server", text: "Input should be a valid UUID" },
      { path: ["corners", 1, "x"], why: "server", text: "Input should be at least 0" },
      { path: [], why: "server", text: "Field required" },
    ]);
  });

  it("are one fault of the whole for a message alone, and none for an answer of another kind", () => {
    expect(refusals({ detail: "The stage is busy." })).toEqual([
      { path: [], why: "server", text: "The stage is busy." },
    ]);
    expect(refusals(null)).toEqual([]);
    expect(refusals({ title: "Not Found" })).toEqual([]);
  });
});

describe("where faults are shown", () => {
  const schema = {
    type: "object",
    properties: {
      region: {
        oneOf: [
          { const: "whole" },
          {
            type: "object",
            properties: { rect: { type: "object", properties: { w: { type: "number" } } } },
            required: ["rect"],
          },
        ],
      },
      gains: { type: ["object", "null"], properties: { red: { type: "number" } } },
      corners: { type: "array", items: { type: "number" } },
      labels: { type: "object", additionalProperties: { type: "string" } },
      pattern: {
        oneOf: [
          {
            type: "object",
            properties: { kind: { const: "spiral" }, turns: { type: "integer" } },
            required: ["kind"],
          },
          { type: "object", properties: { kind: { const: "snake" } }, required: ["kind"] },
        ],
      },
    },
  };
  const shape = shapeOf(schema);
  const value: Json = {
    region: { rect: { w: 1 } },
    gains: null,
    corners: [1, 2],
    labels: { slide: "4" },
    pattern: { kind: "spiral", turns: 2 },
  };

  it("is every place a form draws for the value", () => {
    expect(placesOf(shape, value).map((path) => path.join("/"))).toEqual([
      "",
      "region",
      "region/rect",
      "region/rect/w",
      "gains",
      "corners",
      "corners/0",
      "corners/1",
      "labels",
      "labels/slide",
      "pattern",
      "pattern/kind",
      "pattern/turns",
    ]);
  });

  it("puts each fault at its place, or the nearest drawn around it", () => {
    const fault = (path: Fault["path"]): Fault => ({ path, why: "server", text: path.join("/") });
    const by = placed(
      [
        fault(["corners", 1]),
        fault(["gains", "red"]),
        fault(["region", "rect", "h"]),
        fault(["nowhere"]),
      ],
      placesOf(shape, value),
    );
    expect([...by.entries()].map(([key, here]) => [key, here.map((one) => one.text)])).toEqual([
      [keyOf(["corners", 1]), ["corners/1"]],
      [keyOf(["gains"]), ["gains/red"]],
      [keyOf(["region", "rect"]), ["region/rect/h"]],
      [keyOf([]), ["nowhere"]],
    ]);
  });

  it("reads the value at a place, and compares values", () => {
    expect(valueAt(value, ["region", "rect", "w"])).toBe(1);
    expect(valueAt(value, ["corners", 1])).toBe(2);
    expect(valueAt(value, ["corners", "x"])).toBeUndefined();
    expect(valueAt(value, ["labels", 0])).toBeUndefined();
    expect(same({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] })).toBe(true);
    expect(same({ a: 1 }, { a: 1, b: 2 })).toBe(false);
    expect(same([1], { 0: 1 })).toBe(false);
    expect(same(null, {})).toBe(false);
  });
});
