// The gallery's own schema: a scan's settings, written as schemars writes
// one, with a field of every kind the forms draw, and one they don't.

export const EXAMPLE_SCHEMA = {
  title: "A scan",
  type: "object",
  properties: {
    name: { type: "string", minLength: 1, description: "What the scan is called." },
    project: { type: "string", format: "uuid", description: "The project it is kept in." },
    objective: { $ref: "#/$defs/Objective", default: "10x" },
    exposure_ms: {
      type: "number",
      minimum: 0.1,
      maximum: 1000,
      default: 20,
      unit: "ms",
      description: "How long each frame is exposed.",
    },
    overlap: {
      type: "number",
      exclusiveMinimum: 0,
      maximum: 0.5,
      default: 0.1,
      description: "How much of each tile the next one covers.",
    },
    tiles: { type: "integer", format: "uint32", minimum: 1, maximum: 400, default: 16 },
    autofocus: {
      type: "boolean",
      default: true,
      description: "Each tile is focused before it is taken.",
    },
    region: { $ref: "#/$defs/Region" },
    pattern: { $ref: "#/$defs/Pattern" },
    heights_um: {
      type: "array",
      items: { type: "number", format: "double" },
      minItems: 1,
      maxItems: 5,
      default: [0],
      description: "The heights a stack is taken at, from the focus.",
    },
    white_balance: {
      anyOf: [{ $ref: "#/$defs/Gains" }, { type: "null" }],
      description: "The colour gains, if not the camera's own.",
    },
    labels: {
      type: "object",
      additionalProperties: { type: "string" },
      description: "Words kept with the scan, by name.",
    },
    extra: { description: "Anything else, as JSON." },
    slide: {
      type: "string",
      default: "AB-1234",
      pattern: "^[A-Z]{2}-\\d{4}$",
      description: "The slide's code, such as AB-1234.",
    },
  },
  required: ["name", "project", "region", "pattern", "slide"],
  additionalProperties: false,
  $defs: {
    Objective: {
      oneOf: [
        { type: "string", const: "4x", description: "4×, NA 0.10." },
        { type: "string", const: "10x", description: "10×, NA 0.28." },
        { type: "string", const: "20x", description: "20×, NA 0.42." },
        { type: "string", const: "50x", description: "50×, NA 0.55." },
      ],
    },
    Region: {
      description: "Where the scan goes.",
      oneOf: [
        { type: "string", const: "whole", description: "The whole slide." },
        {
          type: "object",
          properties: { rect: { $ref: "#/$defs/Rect" } },
          required: ["rect"],
          additionalProperties: false,
          description: "A rectangle.",
        },
        {
          type: "object",
          properties: {
            circle: {
              type: "object",
              properties: {
                x: { type: "number" },
                y: { type: "number" },
                radius: { type: "number", exclusiveMinimum: 0, default: 500 },
              },
              required: ["x", "y", "radius"],
            },
          },
          required: ["circle"],
          additionalProperties: false,
          description: "A circle.",
        },
      ],
    },
    Rect: {
      type: "object",
      properties: {
        x: { type: "number", description: "Its left edge, in µm." },
        y: { type: "number", description: "Its top edge, in µm." },
        width: { type: "number", minimum: 0, default: 1000 },
        height: { type: "number", minimum: 0, default: 1000 },
      },
      required: ["x", "y", "width", "height"],
    },
    Pattern: {
      description: "The order the tiles are taken in.",
      oneOf: [
        {
          type: "object",
          properties: {
            kind: { type: "string", const: "raster" },
            rows_first: { type: "boolean", default: true },
          },
          required: ["kind"],
          description: "Row by row, each from the same side.",
        },
        {
          type: "object",
          properties: { kind: { type: "string", const: "snake" } },
          required: ["kind"],
          description: "Row by row, turning at each end.",
        },
        {
          type: "object",
          properties: {
            kind: { type: "string", const: "spiral" },
            turns: { type: "integer", minimum: 1, default: 3 },
          },
          required: ["kind", "turns"],
          description: "Out from the middle.",
        },
      ],
    },
    Gains: {
      type: "object",
      properties: {
        red: { type: "number", minimum: 0, maximum: 4, default: 1 },
        green: { type: "number", minimum: 0, maximum: 4, default: 1 },
        blue: { type: "number", minimum: 0, maximum: 4, default: 1 },
      },
      required: ["red", "green", "blue"],
    },
  },
};
