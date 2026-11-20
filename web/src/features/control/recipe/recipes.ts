// The live recipes that come with the app (W3.7), until W7.1's library
// keeps recipes on the server; and a recipe opened from a file, checked for
// its shape here, and for the rest by the server as it is set (P22).
import type { ValueOf } from "@/api/things/use";

/** A recipe, as the preview takes it. */
export type Recipe = NonNullable<ValueOf<"/api/v1/preview/live_recipe">>;

type Step = { id: string; node: string; params?: Record<string, unknown> };

/** A recipe that runs its steps one after another, each on the last one's image. */
function chain(name: string, steps: Step[]): Recipe {
  return {
    format: 1,
    name,
    inputs: [{ name: "image", type: "image" }],
    nodes: steps.map(({ id, node, params }) => ({ id, node, params: params ?? {} })),
    edges: steps.map((step, i) => ({
      from: i === 0 ? "input.image" : `${steps[i - 1]!.id}.image`,
      to: `${step.id}.image`,
    })),
    outputs: [{ name: "image", from: `${steps.at(-1)!.id}.image` }],
  };
}

/** The flat field's correction, which reads the reference `flat` over the frame's region. */
function flatField(name: string): Recipe {
  const recipe = chain(name, [
    { id: "even", node: "correction.flat_field" },
    { id: "levels", node: "display.levels" },
  ]);
  recipe.inputs.push({ name: "region", type: "json" });
  recipe.nodes.unshift({ id: "flat", node: "io.reference", params: { name: "flat" } });
  recipe.edges.push(
    { from: "flat.image", to: "even.flat" },
    { from: "input.region", to: "even.region" },
  );
  return recipe;
}

/** The recipes that come with the app, by key; their names are the server's. */
export const STOCK: readonly { key: string; recipe: Recipe }[] = [
  {
    key: "levels",
    recipe: chain("Levels and gamma", [
      { id: "levels", node: "display.levels", params: { black: 0.05, white: 0.85, gamma: 2.2 } },
    ]),
  },
  {
    key: "inverted",
    recipe: chain("Inverted", [
      { id: "levels", node: "display.levels" },
      { id: "invert", node: "display.lut", params: { table: [1, 0] } },
    ]),
  },
  {
    key: "green",
    recipe: chain("The green channel alone", [
      { id: "green", node: "convert.format", params: { channels: "green" } },
      { id: "levels", node: "display.levels" },
    ]),
  },
  { key: "flat", recipe: flatField("Flat field") },
];

/** The references a recipe reads, by name, each once. */
export function needsOf(recipe: Recipe): string[] {
  const names = recipe.nodes
    .filter((node) => node.node === "io.reference")
    .map((node) => (node.params as { name?: unknown } | undefined)?.name)
    .filter((name): name is string => typeof name === "string");
  return [...new Set(names)];
}

/** A recipe from a file's text: its shape checked, or an Error that says what is wrong. */
export function recipeFrom(text: string): Recipe {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("it isn't JSON");
  }
  const recipe = parsed as Partial<Recipe> | null;
  if (typeof recipe !== "object" || recipe === null || Array.isArray(recipe)) {
    throw new Error("it isn't a recipe: a recipe is an object");
  }
  if (recipe.format !== 1) throw new Error("its format isn't 1");
  for (const list of ["nodes", "outputs"] as const) {
    if (!Array.isArray(recipe[list]) || recipe[list].length === 0) {
      throw new Error(`it has no ${list}`);
    }
  }
  return {
    ...recipe,
    name: typeof recipe.name === "string" && recipe.name ? recipe.name : "Opened recipe",
    inputs: recipe.inputs ?? [],
    edges: recipe.edges ?? [],
  } as Recipe;
}
