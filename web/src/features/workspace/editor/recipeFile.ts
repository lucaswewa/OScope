// A recipe's file, imported and exported (W7.2): JSON read as a recipe, its parts that
// may be left out filled in. Whether it can run is `check`'s to say; what
// isn't a recipe at all is refused here, saying why.
import type { Recipe } from "@/api/pipeline";

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** `json` as a recipe, or an error saying why it isn't one. */
export function asRecipe(json: unknown): Recipe {
  if (!isRecord(json)) throw new Error("it isn't a recipe: not a JSON object");
  if (typeof json.format !== "number") throw new Error("it isn't a recipe: it has no format");
  for (const part of ["nodes", "outputs"] as const) {
    if (!Array.isArray(json[part])) throw new Error(`it isn't a recipe: it has no ${part}`);
  }
  for (const part of ["inputs", "edges"] as const) {
    if (json[part] !== undefined && !Array.isArray(json[part]))
      throw new Error(`it isn't a recipe: its ${part} aren't a list`);
  }
  if (json.layout !== undefined && !isRecord(json.layout))
    throw new Error("it isn't a recipe: its layout isn't an object");
  for (const [id, at] of Object.entries(json.layout ?? {})) {
    if (!Array.isArray(at) || at.length !== 2 || !at.every((v) => typeof v === "number"))
      throw new Error(`it isn't a recipe: its layout's ${id} isn't a place, [x, y]`);
  }
  // Its shape checked so far; its parts' own, `check`'s to say.
  return { name: "", inputs: [], edges: [], layout: {}, ...json } as unknown as Recipe;
}

/** `recipe` as its file holds it: its parts in one order, the server's, whatever order it has. */
export function fileOf(recipe: Recipe): string {
  const { format, name, inputs, nodes, edges, outputs, layout, ...rest } = recipe;
  const ordered = { format, name, inputs, nodes, edges, outputs, layout, ...rest };
  return `${JSON.stringify(ordered, null, 2)}\n`;
}
