import { describe, expect, it } from "vitest";

import {
  initial,
  innerOf,
  joined,
  nameOf,
  outside,
  shapeOf,
  variantOf,
  type Of,
  type Shape,
} from "@/ui/schema-form/shape";

const as = <K extends Shape["kind"]>(kind: K, shape: Shape): Of<K> => {
  expect(shape.kind).toBe(kind);
  return shape as Of<K>;
};

describe("a schema read into the shapes a form draws", () => {
  it("reads an object's fields, in order, which must be given, and whether others may be", () => {
    const shape = as(
      "object",
      shapeOf({
        type: "object",
        title: "A scan",
        properties: { name: { type: "string" }, tiles: { type: "integer" } },
        required: ["name"],
        additionalProperties: false,
      }),
    );
    expect(shape.title).toBe("A scan");
    expect(shape.closed).toBe(true);
    expect(shape.fields.map((field) => [field.name, field.shape.kind, field.required])).toEqual([
      ["name", "string", true],
      ["tiles", "number", false],
    ]);
    expect(as("object", shapeOf({ type: "object", properties: {} })).closed).toBe(false);
  });

  it("reads a map from an object whose fields are all of one kind", () => {
    const map = as("map", shapeOf({ type: "object", additionalProperties: { type: "number" } }));
    expect(map.values.kind).toBe("number");
    expect(as("map", shapeOf({ type: "object", additionalProperties: true })).values.kind).toBe(
      "any",
    );
    expect(shapeOf({ type: "object" }).kind).toBe("any");
    expect(shapeOf({ type: "object", additionalProperties: false })).toMatchObject({
      kind: "object",
      fields: [],
    });
  });

  it("reads numbers with their ranges and units, and leaves schemars' formats out", () => {
    expect(shapeOf({ type: "integer", format: "uint32", minimum: 1, maximum: 400 })).toEqual({
      kind: "number",
      integer: true,
      min: 1,
      max: 400,
    });
    expect(
      shapeOf({
        type: "number",
        format: "double",
        exclusiveMinimum: 0,
        exclusiveMaximum: 1,
        unit: "ms",
      }),
    ).toEqual({ kind: "number", integer: false, above: 0, below: 1, unit: "ms" });
  });

  it("reads strings, their lengths, and the formats it checks", () => {
    for (const format of ["uuid", "date-time", "date", "ip"]) {
      expect(shapeOf({ type: "string", format })).toEqual({ kind: "string", format });
    }
    expect(shapeOf({ type: "string", format: "email", minLength: 1, maxLength: 64 })).toEqual({
      kind: "string",
      minLength: 1,
      maxLength: 64,
    });
  });

  it("reads a choice written either way: an enum, or constants each with its description", () => {
    expect(as("choice", shapeOf({ enum: ["png", "jpeg", 3] })).options).toEqual([
      { value: "png", label: "png" },
      { value: "jpeg", label: "jpeg" },
      { value: 3, label: "3" },
    ]);
    const written = shapeOf({
      description: "How a rendition is encoded.",
      oneOf: [
        { type: "string", const: "png", description: "PNG, lossless." },
        { type: "string", const: "jpeg", title: "JPEG", description: "JPEG, at quality 90." },
      ],
    });
    expect(written).toEqual({
      kind: "choice",
      description: "How a rendition is encoded.",
      options: [
        { value: "png", label: "png", description: "PNG, lossless." },
        { value: "jpeg", label: "JPEG", description: "JPEG, at quality 90." },
      ],
    });
  });

  it("reads a value that may be left out, written any of three ways", () => {
    const number = { kind: "number", integer: false };
    expect(shapeOf({ type: ["number", "null"], default: 2 })).toEqual({
      kind: "optional",
      inner: number,
      default: 2,
    });
    expect(shapeOf({ oneOf: [{ type: "number" }, { type: "null" }] })).toEqual({
      kind: "optional",
      inner: number,
    });
    const root = { $defs: { Gains: { type: "object", properties: {} } } };
    expect(shapeOf({ anyOf: [{ $ref: "#/$defs/Gains" }, { type: "null" }] }, root)).toMatchObject({
      kind: "optional",
      inner: { kind: "object" },
    });
    // A choice that may be left out, as teta-wot writes it.
    expect(
      shapeOf({ oneOf: [{ oneOf: [{ const: "png" }, { const: "jpeg" }] }, { type: "null" }] }),
    ).toMatchObject({ kind: "optional", inner: { kind: "choice" } });
    expect(shapeOf({ type: "null" })).toEqual({ kind: "constant", value: null });
  });

  it("reads a union whose objects each say their kind in the same field", () => {
    const union = as(
      "union",
      shapeOf({
        oneOf: [
          {
            type: "object",
            description: "Row by row.",
            properties: { kind: { const: "raster" }, rows_first: { type: "boolean" } },
            required: ["kind"],
          },
          { type: "object", properties: { kind: { const: "snake" } }, required: ["kind"] },
        ],
      }),
    );
    expect(union.tag).toBe("kind");
    expect(union.variants.map((kind) => [kind.value, kind.description])).toEqual([
      ["raster", "Row by row."],
      ["snake", undefined],
    ]);
    // The tag is the union's: a kind's fields are the rest.
    expect(as("object", union.variants[0]!.shape).fields.map((field) => field.name)).toEqual([
      "rows_first",
    ]);
  });

  it("reads a union whose objects are named by their one key, and kinds that hold nothing", () => {
    const union = as(
      "union",
      shapeOf({
        oneOf: [
          { type: "string", const: "he", description: "Hematoxylin and eosin." },
          {
            type: "object",
            properties: { vectors: { type: "array", items: { type: "number" } } },
            required: ["vectors"],
          },
          {
            type: "object",
            properties: { rect: { type: "object", properties: {} } },
            required: ["rect"],
          },
        ],
      }),
    );
    expect(union.tag).toBeNull();
    expect(union.variants.map((kind) => [kind.value, kind.unit ?? false, kind.shape.kind])).toEqual(
      [
        ["he", true, "constant"],
        ["vectors", false, "array"],
        ["rect", false, "object"],
      ],
    );
    // One object alone is that object, which may be left out.
    expect(
      shapeOf({
        oneOf: [
          { type: "object", properties: { libraries: { type: "array" } }, required: ["libraries"] },
          { type: "null" },
        ],
      }),
    ).toMatchObject({ kind: "optional", inner: { kind: "object" } });
  });

  it("follows a reference into the schema's definitions, with what stands beside it first", () => {
    const schema = {
      type: "object",
      properties: {
        objective: { $ref: "#/$defs/Objective", default: "10x", description: "The lens." },
        old: { $ref: "#/definitions/Old" },
        escaped: { $ref: "#/$defs/a~1b" },
      },
      $defs: {
        Objective: { enum: ["4x", "10x"], description: "An objective." },
        "a/b": { type: "boolean" },
      },
      definitions: { Old: { type: "string" } },
    };
    const fields = as("object", shapeOf(schema)).fields;
    expect(fields[0]!.shape).toMatchObject({
      kind: "choice",
      default: "10x",
      description: "The lens.",
    });
    expect(fields[1]!.shape.kind).toBe("string");
    expect(fields[2]!.shape.kind).toBe("boolean");
  });

  it("draws what it doesn't know as JSON, and says why", () => {
    const why = (schema: unknown, root?: unknown): string => as("json", shapeOf(schema, root)).why;
    expect(why({ type: "string", pattern: "^[A-Z]{2}$" })).toBe("it uses `pattern`");
    expect(why({ allOf: [{ type: "string" }, { minLength: 1 }] })).toBe(
      "it is all of several schemas",
    );
    expect(why({ type: ["string", "number"] })).toBe("it is any of string, number");
    expect(why({ type: "array", items: [{ type: "number" }, { type: "string" }] })).toBe(
      "its items differ by place",
    );
    expect(why({ $ref: "other.json#/x" })).toBe("a reference outside the schema: other.json#/x");
    expect(why({ $ref: "#/$defs/Missing" })).toBe("a reference to nothing: #/$defs/Missing");
    expect(why({ oneOf: [{ type: "string" }, { type: "number" }] })).toBe(
      "its kinds can't be told apart",
    );
    expect(
      why({ type: "object", properties: { a: {} }, additionalProperties: { type: "string" } }),
    ).toBe("it has fields, and others of a kind");
    expect(why({ minimum: 3 })).toBe("it says what it holds, but not its type");
    expect(why({ type: "tuple" })).toBe("its type is tuple");
    expect(why(7)).toBe("it isn't a schema");
    // A type that holds itself goes no deeper than a form does.
    const tree = {
      $ref: "#/$defs/Node",
      $defs: { Node: { anyOf: [{ $ref: "#/$defs/Node" }, { type: "null" }] } },
    };
    expect(JSON.stringify(shapeOf(tree))).toContain("it holds itself");
  });

  it("draws a value the schema says nothing of as JSON by right, and lists only what it doesn't know", () => {
    expect(shapeOf({})).toEqual({ kind: "any" });
    expect(shapeOf(true)).toEqual({ kind: "any" });
    expect(shapeOf({ description: "Anything." })).toEqual({
      kind: "any",
      description: "Anything.",
    });
    const mixed = shapeOf({
      type: "object",
      properties: {
        extra: {},
        code: { type: "string", pattern: "x" },
        list: { type: "array", items: { not: {} } },
        map: { type: "object", additionalProperties: { type: "string", pattern: "y" } },
        union: { oneOf: [{ type: "null" }, { type: "string", pattern: "z" }] },
      },
    });
    expect(outside(mixed)).toEqual([
      { where: "/code", why: "it uses `pattern`" },
      { where: "/list/[]", why: "it uses `not`" },
      { where: "/map/*", why: "it uses `pattern`" },
      { where: "/union", why: "it uses `pattern`" },
    ]);
    expect(outside(shapeOf({ pattern: "x" }))).toEqual([
      { where: "(the whole)", why: "it uses `pattern`" },
    ]);
  });

  it("names a field in words, with a unit its name ends in", () => {
    expect(nameOf("name")).toBe("Name");
    expect(nameOf("rows_first")).toBe("Rows first");
    expect(nameOf("exposure_ms")).toBe("Exposure (ms)");
    expect(nameOf("heights_um")).toBe("Heights (µm)");
    expect(nameOf("um")).toBe("Um");
  });
});

describe("the value a form starts from", () => {
  it("is the schema's default, a copy of it, wherever one is given", () => {
    const list = [1, 2];
    const shape = shapeOf({ type: "array", items: { type: "number" }, default: list });
    const value = initial(shape) as number[];
    expect(value).toEqual([1, 2]);
    expect(value).not.toBe(list);
  });

  it("is the least value of each kind, and leaves out fields that may be left out", () => {
    const shape = shapeOf({
      type: "object",
      properties: {
        name: { type: "string" },
        tiles: { type: "integer", minimum: 1 },
        drift: { type: "number", maximum: -2 },
        above: { type: "number", exclusiveMinimum: 0 },
        below: { type: "number", exclusiveMaximum: 0 },
        zero: { type: "number", minimum: -5, maximum: 5 },
        lens: { enum: ["4x", "10x"] },
        on: { type: "boolean" },
        corners: { type: "array", items: { type: "number", minimum: 3 }, minItems: 2 },
        labels: { type: "object", additionalProperties: { type: "string" } },
        fixed: { const: "v1" },
        anything: {},
        note: { type: ["string", "null"] },
        kept: { type: "number", default: 7 },
        left: { type: "number" },
      },
      required: [
        "name",
        "tiles",
        "drift",
        "above",
        "below",
        "zero",
        "lens",
        "on",
        "corners",
        "labels",
        "fixed",
        "anything",
        "note",
      ],
    });
    expect(initial(shape)).toEqual({
      name: "",
      tiles: 1,
      drift: -2,
      above: 1,
      below: -1,
      zero: 0,
      lens: "4x",
      on: false,
      corners: [3, 3],
      labels: {},
      fixed: "v1",
      anything: null,
      note: null,
      kept: 7,
    });
  });

  it("is a union's first kind, written as its tag or its key says", () => {
    const tagged = shapeOf({
      oneOf: [
        {
          type: "object",
          properties: { kind: { const: "spiral" }, turns: { type: "integer", minimum: 3 } },
          required: ["kind", "turns"],
        },
        { type: "object", properties: { kind: { const: "snake" } }, required: ["kind"] },
      ],
    });
    expect(initial(tagged)).toEqual({ kind: "spiral", turns: 3 });
    const keyed = shapeOf({
      oneOf: [
        {
          type: "object",
          properties: {
            rect: { type: "object", properties: { w: { type: "number" } }, required: ["w"] },
          },
          required: ["rect"],
        },
        { const: "whole" },
      ],
    });
    expect(initial(keyed)).toEqual({ rect: { w: 0 } });
  });
});

describe("a union's value", () => {
  const union = as(
    "union",
    shapeOf({
      oneOf: [
        { const: "whole" },
        {
          type: "object",
          properties: { rect: { type: "object", properties: {} } },
          required: ["rect"],
        },
      ],
    }),
  );
  const tagged = as(
    "union",
    shapeOf({
      oneOf: [
        {
          type: "object",
          properties: { kind: { const: "a" }, n: { type: "number" } },
          required: ["kind"],
        },
        { type: "object", properties: { kind: { const: "b" } }, required: ["kind"] },
      ],
    }),
  );

  it("is put together from its kind and what that holds", () => {
    expect(joined(union, union.variants[0]!, null)).toBe("whole");
    expect(joined(union, union.variants[1]!, { w: 3 })).toEqual({ rect: { w: 3 } });
    expect(joined(tagged, tagged.variants[0]!, { n: 2 })).toEqual({ kind: "a", n: 2 });
    expect(joined(tagged, tagged.variants[1]!, null)).toEqual({ kind: "b" });
  });

  it("says its kind, and what it holds", () => {
    expect(variantOf(union, "whole")?.value).toBe("whole");
    expect(variantOf(union, { rect: {} })?.value).toBe("rect");
    expect(variantOf(union, { rect: {}, more: 1 })).toBeUndefined();
    expect(variantOf(union, { whole: {} })).toBeUndefined();
    expect(variantOf(union, "nothing")).toBeUndefined();
    expect(variantOf(tagged, { kind: "b" })?.value).toBe("b");
    expect(variantOf(tagged, { kind: "c" })).toBeUndefined();
    expect(variantOf(tagged, [1])).toBeUndefined();
    expect(innerOf(union, { rect: { w: 1 } })).toEqual({ w: 1 });
    expect(innerOf(tagged, { kind: "a", n: 2 })).toEqual({ n: 2 });
    expect(innerOf(tagged, 5)).toBeNull();
  });
});
