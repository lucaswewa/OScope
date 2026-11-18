// A JSON Schema, read into the shapes a form draws (W0.6): the part of JSON
// Schema the server's schemas use, as schemars writes them, and as teta-wot
// turns them into a Thing Description's data schemas. Anything else is drawn
// as JSON to edit, with the reason, and never as nothing.

/** A JSON value. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** A string's formats that are checked. Others are only shown. */
export type Format = "uuid" | "date-time" | "date" | "ip";

/** A choice's option: its value, and what it is called. */
export interface Option {
  value: Json;
  label: string;
  description?: string;
}

/** A union's kind: what its tag says, and its fields, the tag apart. */
export interface Variant {
  value: string;
  label: string;
  description?: string;
  /** It holds nothing: its value is its name alone, as a string. */
  unit?: true;
  shape: Shape;
}

/** An object's field. */
export interface Field {
  name: string;
  shape: Shape;
  /** It must be given. A field that needn't be is drawn as one that may be left out. */
  required: boolean;
}

interface About {
  title?: string;
  description?: string;
  /** The schema's own default, where it gives one. */
  default?: Json;
}

export type Shape = About &
  (
    | { kind: "object"; fields: Field[]; closed: boolean }
    | { kind: "map"; values: Shape }
    | {
        kind: "number";
        integer: boolean;
        min?: number;
        max?: number;
        /** What it must be more than. */
        above?: number;
        /** What it must be less than. */
        below?: number;
        unit?: string;
      }
    | { kind: "string"; format?: Format; minLength?: number; maxLength?: number }
    | { kind: "choice"; options: Option[] }
    | { kind: "boolean" }
    | { kind: "array"; items: Shape; minItems?: number; maxItems?: number }
    | { kind: "optional"; inner: Shape }
    /** A tag inside the object names its kind; or, with no tag, its one key does. */
    | { kind: "union"; tag: string | null; variants: Variant[] }
    | { kind: "constant"; value: Json }
    /** A value the schema says nothing of: JSON, by right. */
    | { kind: "any" }
    /** Outside the subset: JSON to edit, and why. */
    | { kind: "json"; why: string }
  );

/** The shape of one kind. */
export type Of<K extends Shape["kind"]> = Extract<Shape, { kind: K }>;

type Schema = Record<string, unknown>;

/** Keywords that say something of the value only to people, or to the Thing, and change no shape. */
const ANNOTATIONS = new Set([
  "$schema",
  "$id",
  "$comment",
  "$defs",
  "definitions",
  "title",
  "description",
  "default",
  "examples",
  "deprecated",
  "readOnly",
  "writeOnly",
  "format",
  "unit",
  "@type",
  "titles",
  "descriptions",
  "contentMediaType",
  "contentEncoding",
]);

/** Keywords each shape reads. */
const READ = new Set([
  "type",
  "properties",
  "required",
  "additionalProperties",
  "items",
  "minItems",
  "maxItems",
  "minimum",
  "maximum",
  "exclusiveMinimum",
  "exclusiveMaximum",
  "minLength",
  "maxLength",
  "enum",
  "const",
  "oneOf",
  "anyOf",
  "allOf",
  "$ref",
]);

/** How deep references and nesting may go: a type that holds itself would never end. */
const DEPTH = 64;

/** An object without one of its keys. */
export function without<T>(object: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(object).filter(([name]) => name !== key));
}

const isSchema = (value: unknown): value is Schema =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Units a name may end in, as they are written. */
const UNITS: Record<string, string> = {
  um: "µm",
  nm: "nm",
  mm: "mm",
  ms: "ms",
  us: "µs",
  s: "s",
  px: "px",
  deg: "°",
};

/** The words for a name: `exposure_ms` is "Exposure (ms)". */
export function nameOf(key: string): string {
  const words = key.split("_").filter(Boolean);
  const unit = words.length > 1 ? UNITS[words.at(-1)!] : undefined;
  const text = (unit ? words.slice(0, -1) : words).join(" ");
  const named = text.charAt(0).toUpperCase() + text.slice(1);
  return unit ? `${named} (${unit})` : named;
}

/** The shape of `schema`, whose `$ref`s point into `root`: itself, unless given. */
export function shapeOf(schema: unknown, root: unknown = schema): Shape {
  return read(schema === true ? {} : schema, isSchema(root) ? root : {}, 0);
}

function about(schema: Schema): About {
  const said: About = {};
  if (typeof schema.title === "string") said.title = schema.title;
  if (typeof schema.description === "string") said.description = schema.description;
  if (schema.default !== undefined && schema.default !== null)
    said.default = schema.default as Json;
  return said;
}

/** The schema a local `$ref` names, with what stands beside the `$ref` over it. */
function resolve(schema: Schema, root: Schema): Schema | string {
  const target = schema.$ref;
  if (typeof target !== "string" || !target.startsWith("#")) {
    return `a reference outside the schema: ${String(target)}`;
  }
  let at: unknown = root;
  for (const part of target.slice(1).split("/").filter(Boolean)) {
    const key = part.replace(/~1/g, "/").replace(/~0/g, "~");
    at = isSchema(at) ? at[key] : undefined;
  }
  if (at === true) at = {};
  if (!isSchema(at)) return `a reference to nothing: ${target}`;
  const beside = without(schema, "$ref");
  return { ...at, ...beside };
}

function read(given: unknown, root: Schema, depth: number): Shape {
  if (depth > DEPTH) return { kind: "json", why: "it holds itself, deeper than a form goes" };
  if (!isSchema(given)) return { kind: "json", why: "it isn't a schema" };
  let schema = given;
  if ("$ref" in schema) {
    const found = resolve(schema, root);
    if (typeof found === "string") return { ...about(schema), kind: "json", why: found };
    return read(found, root, depth + 1);
  }
  const unknown = Object.keys(schema).find((key) => !ANNOTATIONS.has(key) && !READ.has(key));
  if (unknown) return { ...about(schema), kind: "json", why: `it uses \`${unknown}\`` };
  if (Array.isArray(schema.allOf)) {
    if (schema.allOf.length !== 1 || !isSchema(schema.allOf[0])) {
      return { ...about(schema), kind: "json", why: "it is all of several schemas" };
    }
    const { allOf, ...rest } = schema;
    schema = { ...(allOf[0] as Schema), ...rest };
    return read(schema, root, depth + 1);
  }
  const said = about(schema);
  const branches = schema.oneOf ?? schema.anyOf;
  if (Array.isArray(branches)) return { ...choiceOf(branches, root, depth), ...said };
  if ("const" in schema) return { ...said, kind: "constant", value: schema.const as Json };
  if (Array.isArray(schema.enum)) {
    return {
      ...choiceOf(
        schema.enum.map((value) => ({ const: value })),
        root,
        depth,
      ),
      ...said,
    };
  }
  const type = schema.type;
  if (Array.isArray(type)) {
    const others = type.filter((one) => one !== "null");
    if (others.length !== 1) {
      return { ...said, kind: "json", why: `it is any of ${type.join(", ")}` };
    }
    const plain = without(schema, "default");
    const inner = read({ ...plain, type: others[0] }, root, depth + 1);
    return others.length < type.length ? { ...said, kind: "optional", inner } : inner;
  }
  switch (type) {
    case undefined:
      return Object.keys(schema).some((key) => READ.has(key))
        ? { ...said, kind: "json", why: "it says what it holds, but not its type" }
        : { ...said, kind: "any" };
    case "null":
      return { ...said, kind: "constant", value: null };
    case "boolean":
      return { ...said, kind: "boolean" };
    case "integer":
    case "number":
      return { ...said, ...numberOf(schema, type === "integer") };
    case "string":
      return { ...said, ...stringOf(schema) };
    case "array":
      return { ...said, ...arrayOf(schema, root, depth) };
    case "object":
      return { ...said, ...objectOf(schema, root, depth) };
    default:
      return { ...said, kind: "json", why: `its type is ${String(type)}` };
  }
}

const numeric = (value: unknown): number | undefined =>
  typeof value === "number" && Number.isFinite(value) ? value : undefined;

function numberOf(schema: Schema, integer: boolean): Of<"number"> {
  const shape: Of<"number"> = { kind: "number", integer };
  const min = numeric(schema.minimum);
  const max = numeric(schema.maximum);
  const above = numeric(schema.exclusiveMinimum);
  const below = numeric(schema.exclusiveMaximum);
  if (min !== undefined) shape.min = min;
  if (max !== undefined) shape.max = max;
  if (above !== undefined) shape.above = above;
  if (below !== undefined) shape.below = below;
  if (typeof schema.unit === "string") shape.unit = schema.unit;
  return shape;
}

function stringOf(schema: Schema): Of<"string"> {
  const shape: Of<"string"> = { kind: "string" };
  const format = schema.format;
  if (format === "uuid" || format === "date-time" || format === "date" || format === "ip") {
    shape.format = format;
  }
  const least = numeric(schema.minLength);
  const most = numeric(schema.maxLength);
  if (least !== undefined) shape.minLength = least;
  if (most !== undefined) shape.maxLength = most;
  return shape;
}

function arrayOf(schema: Schema, root: Schema, depth: number): Shape {
  if (Array.isArray(schema.items)) return { kind: "json", why: "its items differ by place" };
  const shape: Of<"array"> = {
    kind: "array",
    items: read(schema.items === undefined ? {} : schema.items, root, depth + 1),
  };
  const least = numeric(schema.minItems);
  const most = numeric(schema.maxItems);
  if (least !== undefined) shape.minItems = least;
  if (most !== undefined) shape.maxItems = most;
  return shape;
}

function objectOf(schema: Schema, root: Schema, depth: number): Shape {
  const properties = isSchema(schema.properties) ? schema.properties : undefined;
  const extra = schema.additionalProperties;
  if (!properties) {
    if (isSchema(extra) || extra === true) {
      return { kind: "map", values: read(extra === true ? {} : extra, root, depth + 1) };
    }
    return extra === false ? { kind: "object", fields: [], closed: true } : { kind: "any" };
  }
  if (isSchema(extra)) return { kind: "json", why: "it has fields, and others of a kind" };
  const required = new Set(Array.isArray(schema.required) ? schema.required : []);
  const fields = Object.entries(properties).map(([name, value]) => ({
    name,
    shape: read(value, root, depth + 1),
    required: required.has(name),
  }));
  return { kind: "object", fields, closed: extra === false };
}

/** A choice between schemas: values, one shape or none, or a union of objects. */
function choiceOf(branches: unknown[], root: Schema, depth: number): Shape {
  const shapes = branches.map((branch) => read(branch, root, depth + 1));
  const isNull = (shape: Shape): boolean => shape.kind === "constant" && shape.value === null;
  const rest = shapes.filter((shape) => !isNull(shape));
  const optional = rest.length < shapes.length;
  const wrap = (inner: Shape): Shape => (optional ? { kind: "optional", inner } : inner);
  if (rest.length === 0) return { kind: "constant", value: null };
  if (rest.length === 1) return wrap(rest[0]!);
  if (rest.every((shape) => shape.kind === "constant" || shape.kind === "choice")) {
    const options = rest.flatMap((shape): Option[] => {
      if (shape.kind === "choice") return shape.options;
      if (shape.kind !== "constant") return [];
      const option: Option = { value: shape.value, label: shape.title ?? String(shape.value) };
      if (shape.description) option.description = shape.description;
      return [option];
    });
    return wrap({ kind: "choice", options });
  }
  const union = unionOf(rest);
  return union ? wrap(union) : { kind: "json", why: "its kinds can't be told apart" };
}

/** Objects told apart by a field every one has as a constant, or by their one key. */
function unionOf(shapes: Shape[]): Of<"union"> | undefined {
  const objects = shapes.filter((shape): shape is Of<"object"> => shape.kind === "object");
  // Kinds that hold nothing are their names alone, beside those that hold something.
  const units = shapes.filter(
    (shape): shape is Of<"constant"> =>
      shape.kind === "constant" && typeof shape.value === "string",
  );
  if (objects.length + units.length !== shapes.length || objects.length === 0) return undefined;
  const tagged = (field: Field): boolean =>
    field.required && field.shape.kind === "constant" && typeof field.shape.value === "string";
  const tag = objects[0]!.fields.find(
    (field) =>
      tagged(field) &&
      objects.every((one) => one.fields.some((f) => f.name === field.name && tagged(f))),
  )?.name;
  if (tag !== undefined && units.length === 0) {
    const variants = objects.map((object) => {
      const value = object.fields.find((field) => field.name === tag)!.shape as { value: string };
      return variant(value.value, object, {
        ...object,
        fields: object.fields.filter((field) => field.name !== tag),
      });
    });
    return distinct(variants) ? { kind: "union", tag, variants } : undefined;
  }
  const single = objects.every(
    (object) => object.fields.length === 1 && object.fields[0]!.required,
  );
  if (!single) return undefined;
  // In the schema's order: its first kind is the one a form starts from.
  const variants = shapes.map((shape): Variant => {
    if (shape.kind !== "object") {
      return { ...variant(String((shape as Of<"constant">).value), shape, shape), unit: true };
    }
    const field = shape.fields[0]!;
    return variant(field.name, shape, field.shape);
  });
  return distinct(variants) ? { kind: "union", tag: null, variants } : undefined;
}

function variant(value: string, object: Shape, shape: Shape): Variant {
  const made: Variant = { value, label: value, shape };
  const description = object.description ?? object.title;
  if (description) made.description = description;
  return made;
}

const distinct = (variants: Variant[]): boolean =>
  new Set(variants.map((one) => one.value)).size === variants.length;

/**
 * A value of the shape, to start from: the schema's default, or else the
 * least that is a value of it. A field that may be left out is left out.
 */
export function initial(shape: Shape): Json {
  if (shape.default !== undefined) return structuredClone(shape.default);
  switch (shape.kind) {
    case "object": {
      const value: Record<string, Json> = {};
      for (const field of shape.fields) {
        if (field.required || field.shape.default !== undefined)
          value[field.name] = initial(field.shape);
      }
      return value;
    }
    case "map":
      return {};
    case "number": {
      const least = shape.min ?? (shape.above === undefined ? undefined : shape.above + 1);
      const most = shape.max ?? (shape.below === undefined ? undefined : shape.below - 1);
      return least !== undefined && least > 0 ? least : most !== undefined && most < 0 ? most : 0;
    }
    case "string":
      return "";
    case "choice":
      return structuredClone(shape.options[0]?.value ?? null);
    case "boolean":
      return false;
    case "array":
      return Array.from({ length: shape.minItems ?? 0 }, () => initial(shape.items));
    case "union":
      return joined(shape, shape.variants[0]!, initial(shape.variants[0]!.shape));
    case "constant":
      return structuredClone(shape.value);
    default:
      return null;
  }
}

/** A union's value of one kind: the tag beside the kind's fields, or its one key around its value. */
export function joined(union: Of<"union">, kind: Variant, inner: Json): Json {
  if (kind.unit) return kind.value;
  if (union.tag === null) return { [kind.value]: inner };
  const fields = typeof inner === "object" && inner !== null && !Array.isArray(inner) ? inner : {};
  return { [union.tag]: kind.value, ...fields };
}

/** Which of a union's kinds a value is, if any. */
export function variantOf(union: Of<"union">, value: Json): Variant | undefined {
  if (typeof value === "string")
    return union.variants.find((kind) => kind.unit && kind.value === value);
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const name = union.tag === null ? Object.keys(value)[0] : value[union.tag];
  if (union.tag === null && Object.keys(value).length !== 1) return undefined;
  return union.variants.find((kind) => !kind.unit && kind.value === name);
}

/** What a union's value of a kind holds: its fields, the tag apart, or its one key's value. */
export function innerOf(union: Of<"union">, value: Json): Json {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  if (union.tag === null) return Object.values(value)[0] ?? null;
  return without(value, union.tag);
}

/** Every value a shape draws as JSON because it is outside the subset, by where it is. */
export function outside(shape: Shape, where = ""): { where: string; why: string }[] {
  switch (shape.kind) {
    case "json":
      return [{ where: where || "(the whole)", why: shape.why }];
    case "object":
      return shape.fields.flatMap((field) => outside(field.shape, `${where}/${field.name}`));
    case "map":
      return outside(shape.values, `${where}/*`);
    case "array":
      return outside(shape.items, `${where}/[]`);
    case "optional":
      return outside(shape.inner, where);
    case "union":
      return shape.variants.flatMap((kind) => outside(kind.shape, `${where}/${kind.value}`));
    default:
      return [];
  }
}
