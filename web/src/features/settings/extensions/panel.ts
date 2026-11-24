// An extension's panel (W8.5, ADR-0036): its description read and checked
// for what the app draws, and what its submission answered, as its view
// shows it. A panel is data: the app runs no code from an extension.
import { shapeOf, type Shape } from "@/ui/schema-form/shape";

/** What submitting a panel's form does. */
export type Submit =
  { kind: "node"; node: string } | { kind: "action"; thing: string; action: string };
export type View = "image" | "table" | "json" | "text";
const VIEWS: readonly View[] = ["image", "table", "json", "text"];

/** A panel, as the app draws it. */
export interface Panel {
  title: string;
  description: string;
  form: unknown;
  submit: Submit;
  view: View;
}

/** A panel read: it, if the app can draw it; and what in it the app doesn't know, in words. */
export interface Read {
  panel?: Panel | undefined;
  unknown: string[];
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === "string" && value !== "";

/** What of `shape`'s fields the form can only show as JSON, and why, by their names. */
function unshaped(shape: Shape, path: string): string[] {
  switch (shape.kind) {
    case "json":
      return [`${path || "the form"}: ${shape.why}`];
    case "object":
      return shape.fields.flatMap((field) =>
        unshaped(field.shape, path ? `${path}.${field.name}` : field.name),
      );
    case "optional":
      return unshaped(shape.inner, path);
    case "array":
      return unshaped(shape.items, `${path}[]`);
    case "map":
      return unshaped(shape.values, `${path}.*`);
    default:
      return [];
  }
}

/** `description`, a panel as the server gives it, read: what the app draws, and what it doesn't know. */
export function readPanel(description: unknown): Read {
  if (!isObject(description)) return { unknown: ["it isn't a panel's description"] };
  const unknown: string[] = [];
  const { title, form, submit, view } = description;
  if (!text(title)) unknown.push("it has no title");
  const shape = shapeOf(form);
  if (shape.kind !== "object") unknown.push("its form isn't of an object");
  else unknown.push(...unshaped(shape, ""));
  let submitted: Submit | undefined;
  if (isObject(submit) && submit.kind === "node" && text(submit.node)) {
    submitted = { kind: "node", node: submit.node };
  } else if (
    isObject(submit) &&
    submit.kind === "action" &&
    text(submit.thing) &&
    text(submit.action)
  ) {
    submitted = { kind: "action", thing: submit.thing, action: submit.action };
  } else {
    const kind = isObject(submit) ? String(submit.kind) : "none";
    unknown.push(`its submission, of the kind \`${kind}\`, isn't a node's or an action's`);
  }
  const shown = VIEWS.find((one) => one === view);
  if (!shown) unknown.push(`its view, \`${String(view)}\`, isn't an image, a table, JSON or text`);
  if (!text(title) || shape.kind !== "object" || !submitted || !shown) return { unknown };
  const said = typeof description.description === "string" ? description.description : "";
  return { panel: { title, description: said, form, submit: submitted, view: shown }, unknown };
}

/** What a view shows of an answer. */
export type Shown =
  | { kind: "image"; src: string; width: number; height: number; port: string }
  | { kind: "table"; columns: string[]; rows: unknown[][]; port: string }
  | { kind: "text"; text: string }
  | { kind: "nothing" };

/**
 * `answer`, what a submission gave, as `view` shows it. A node's answer is
 * each of its outputs by port (`preview/run_node`): the first image, or
 * table, that the view asks for. An action's is its output.
 */
export function shownOf(view: View, answer: unknown): Shown {
  const outputs = isObject(answer) ? Object.entries(answer) : [];
  if (view === "image") {
    for (const [port, value] of outputs) {
      const image = isObject(value) ? value.image : undefined;
      if (isObject(image) && typeof image.png === "string") {
        const [width, height] = [Number(image.width), Number(image.height)];
        return { kind: "image", src: `data:image/png;base64,${image.png}`, width, height, port };
      }
    }
    return { kind: "nothing" };
  }
  if (view === "table") {
    for (const [port, value] of outputs) {
      const table = isObject(value) && isObject(value.table) ? value.table : value;
      if (isObject(table) && Array.isArray(table.columns) && Array.isArray(table.rows)) {
        return {
          kind: "table",
          columns: table.columns.map(String),
          rows: table.rows as unknown[][],
          port,
        };
      }
    }
    return { kind: "nothing" };
  }
  if (view === "text" && typeof answer === "string") return { kind: "text", text: answer };
  if (
    view === "text" &&
    outputs.length > 0 &&
    outputs.every(([, value]) => typeof value === "string")
  ) {
    return { kind: "text", text: outputs.map(([, value]) => value).join("\n") };
  }
  return answer === undefined
    ? { kind: "nothing" }
    : { kind: "text", text: JSON.stringify(answer, null, 2) };
}
