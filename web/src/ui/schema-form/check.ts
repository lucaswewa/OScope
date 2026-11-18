// A value checked against its shape (W0.6), and the server's refusals read
// as faults on the fields they name.
import { innerOf, variantOf, type Json, type Shape } from "./shape";

/** Where a value is, from the whole: field names and places in lists. */
export type Path = (string | number)[];

/** What is wrong, as a word of `ui.form` and what it says, or as the server put it. */
export interface Fault {
  path: Path;
  why: string;
  values?: Record<string, string | number>;
  /** The server's own words, for a refusal. */
  text?: string;
}

/** A path as a key: the same path, the same key. */
export const keyOf = (path: Path): string => JSON.stringify(path);

const PATTERNS: Record<string, RegExp> = {
  uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
  "date-time": /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/,
  date: /^\d{4}-\d{2}-\d{2}$/,
  ip: /^((25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(25[0-5]|2[0-4]\d|1?\d?\d)$|^[0-9a-f:]*:[0-9a-f:.]*$/i,
};

/** The word each format's fault is said by. */
const FORMATS: Record<string, string> = {
  uuid: "uuid",
  "date-time": "dateTime",
  date: "date",
  ip: "ip",
};

const isObject = (value: unknown): value is Record<string, Json> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Whether two JSON values are the same. */
export function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  return keys.every((key) =>
    same((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
  );
}

/** Every fault of `value` as a value of `shape`. */
export function faultsOf(shape: Shape, value: Json | undefined, path: Path = []): Fault[] {
  const fault = (why: string, values?: Fault["values"]): Fault[] => [
    values ? { path, why, values } : { path, why },
  ];
  if (value === undefined) return fault("needed");
  switch (shape.kind) {
    case "object": {
      if (!isObject(value)) return fault("object");
      const known = new Set(shape.fields.map((field) => field.name));
      return [
        ...shape.fields.flatMap((field) =>
          field.name in value
            ? faultsOf(field.shape, value[field.name], [...path, field.name])
            : field.required
              ? [{ path: [...path, field.name], why: "needed" }]
              : [],
        ),
        ...(shape.closed
          ? Object.keys(value)
              .filter((key) => !known.has(key))
              .map((key) => ({ path: [...path, key], why: "unknown" }))
          : []),
      ];
    }
    case "map":
      if (!isObject(value)) return fault("object");
      return Object.entries(value).flatMap(([key, one]) =>
        faultsOf(shape.values, one, [...path, key]),
      );
    case "number": {
      if (typeof value !== "number" || !Number.isFinite(value)) return fault("number");
      if (shape.integer && !Number.isInteger(value)) return fault("whole");
      if (shape.min !== undefined && value < shape.min) return fault("least", { least: shape.min });
      if (shape.max !== undefined && value > shape.max) return fault("most", { most: shape.max });
      if (shape.above !== undefined && value <= shape.above)
        return fault("above", { above: shape.above });
      if (shape.below !== undefined && value >= shape.below)
        return fault("below", { below: shape.below });
      return [];
    }
    case "string": {
      if (typeof value !== "string") return fault("text");
      const length = [...value].length;
      if (shape.minLength !== undefined && length < shape.minLength) {
        return shape.minLength === 1
          ? fault("needed")
          : fault("shortest", { count: shape.minLength });
      }
      if (shape.maxLength !== undefined && length > shape.maxLength) {
        return fault("longest", { count: shape.maxLength });
      }
      if (shape.format && !PATTERNS[shape.format]!.test(value))
        return fault(FORMATS[shape.format]!);
      return [];
    }
    case "choice":
      return shape.options.some((option) => same(option.value, value)) ? [] : fault("choice");
    case "boolean":
      return typeof value === "boolean" ? [] : fault("boolean");
    case "array": {
      if (!Array.isArray(value)) return fault("list");
      if (shape.minItems !== undefined && value.length < shape.minItems) {
        return fault("fewest", { count: shape.minItems });
      }
      if (shape.maxItems !== undefined && value.length > shape.maxItems) {
        return fault("mostItems", { count: shape.maxItems });
      }
      return value.flatMap((item, at) => faultsOf(shape.items, item, [...path, at]));
    }
    case "optional":
      return value === null ? [] : faultsOf(shape.inner, value, path);
    case "union": {
      const kind = variantOf(shape, value);
      if (!kind)
        return [{ path: shape.tag === null ? path : [...path, shape.tag], why: "variant" }];
      if (kind.unit) return [];
      const inner = innerOf(shape, value);
      return faultsOf(kind.shape, inner, shape.tag === null ? [...path, kind.value] : path);
    }
    case "constant":
      return same(value, shape.value)
        ? []
        : fault("constant", { value: JSON.stringify(shape.value) });
    default:
      return [];
  }
}

/**
 * The faults in a server's refusal: a 422's `{"detail": [{"loc", "msg"}]}`,
 * whose places start at the request's body; or one message, `{"detail": "…"}`,
 * for the whole.
 */
export function refusals(answer: unknown): Fault[] {
  const detail = isObject(answer) ? answer.detail : undefined;
  if (typeof detail === "string") return [{ path: [], why: "server", text: detail }];
  if (!Array.isArray(detail)) return [];
  return detail.flatMap((issue): Fault[] => {
    if (!isObject(issue) || typeof issue.msg !== "string") return [];
    const loc = Array.isArray(issue.loc) ? issue.loc : [];
    const path = loc.filter(
      (at): at is string | number => typeof at === "string" || typeof at === "number",
    );
    if (path[0] === "body") path.shift();
    return [{ path, why: "server", text: issue.msg }];
  });
}

/** Every place a form draws for a value of a shape: where its faults can be shown. */
export function placesOf(shape: Shape, value: Json | undefined, path: Path = []): Path[] {
  const here = [path];
  switch (shape.kind) {
    case "object":
      return [
        ...here,
        ...shape.fields.flatMap((field) =>
          placesOf(field.shape, isObject(value) ? value[field.name] : undefined, [
            ...path,
            field.name,
          ]),
        ),
      ];
    case "map":
      return isObject(value)
        ? [
            ...here,
            ...Object.entries(value).flatMap(([key, one]) =>
              placesOf(shape.values, one, [...path, key]),
            ),
          ]
        : here;
    case "array":
      return Array.isArray(value)
        ? [...here, ...value.flatMap((item, at) => placesOf(shape.items, item, [...path, at]))]
        : here;
    case "optional":
      return value === null || value === undefined ? here : placesOf(shape.inner, value, path);
    case "union": {
      const kind = value === undefined ? undefined : variantOf(shape, value);
      const tag = shape.tag === null ? [] : [[...path, shape.tag]];
      if (!kind || kind.unit || value === undefined) return [...here, ...tag];
      // A tagged kind's fields are drawn in the union's own place, which is already here.
      const inner = placesOf(
        kind.shape,
        innerOf(shape, value),
        shape.tag === null ? [...path, kind.value] : path,
      );
      return [...here, ...tag, ...(shape.tag === null ? inner : inner.slice(1))];
    }
    default:
      return here;
  }
}

/**
 * Faults by the place each is shown: its own, or the nearest place around it
 * that is drawn. What no place holds is the whole form's.
 */
export function placed(faults: Fault[], places: Path[]): Map<string, Fault[]> {
  const drawn = new Set(places.map(keyOf));
  const by = new Map<string, Fault[]>();
  for (const fault of faults) {
    let path = fault.path;
    while (path.length > 0 && !drawn.has(keyOf(path))) path = path.slice(0, -1);
    const key = keyOf(path);
    by.set(key, [...(by.get(key) ?? []), fault]);
  }
  return by;
}

/** The value at a path, if there is one. */
export function valueAt(value: Json | undefined, path: Path): Json | undefined {
  let at: Json | undefined = value;
  for (const step of path) {
    if (typeof step === "number" ? !Array.isArray(at) : !isObject(at)) return undefined;
    at = (at as Record<string | number, Json>)[step];
  }
  return at;
}
