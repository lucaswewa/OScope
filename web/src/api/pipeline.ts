// Recipes over the API (W7.1, ADR-0052): the `pipeline` Thing, for the
// graph editor. Its catalogue and its actions go through the generated
// types; its library's routes, which the OpenAPI document doesn't
// describe, are typed here, and checked against a real server by the
// contract tests.
import { act, type Following } from "./actions.ts";
import { fetched, server, type Limits, type Schema, type Server } from "./client.ts";
import type { JobRef } from "./jobs.ts";

type On = Limits & { on?: Server | undefined };

/** A node a recipe may use: its ports, and its parameters' schema. */
export type NodeDescriptor = Schema<"pipeline_nodes_value">[number];
/** A recipe (ADR-0017). */
export type Recipe = Schema<"pipeline_save_recipe_input">["recipe"];
/** One of a recipe's faults: where it is, the node and port it is at, and what. */
export type Fault = Schema<"pipeline_check_output">["faults"][number];
/** The recipe a run uses: the library's, by name, or one given. */
export type WhichRecipe = Schema<"pipeline_run_input">["recipe"];
/** A record a recipe runs on. */
export type RunRecord = Schema<"pipeline_run_input">["records"][number];

/** A recipe in the library, as it is listed. */
export interface ListedRecipe {
  /** Its name in the library. */
  name: string;
  /** The recipe's own name, for people. */
  title: string;
  /** Whether it is OScope's own, which can't be changed. */
  stock: boolean;
}

/** A recipe in the library. */
export interface KeptRecipe {
  name: string;
  stock: boolean;
  recipe: Recipe;
}

/** The nodes recipes may use: the stock ones and the extensions'. */
export function pipelineNodes(options: On = {}): Promise<NodeDescriptor[]> {
  const address = (options.on ?? server).address("/api/v1/pipeline/nodes");
  return fetched(address, (answer) => answer.json() as Promise<NodeDescriptor[]>, options);
}

/**
 * Every fault of `recipe` at once, each where it is: none if it can run. It
 * may be anything: JSON that isn't a recipe is one fault, at `recipe`.
 */
export async function checkRecipe(recipe: unknown, options: Following = {}): Promise<Fault[]> {
  return (await act("/api/v1/pipeline/check", { recipe }, options)).faults;
}

/** The library's recipes: the stock ones first, each by name. */
export function recipeList(options: On = {}): Promise<ListedRecipe[]> {
  const address = (options.on ?? server).address("/api/v1/pipeline/recipes");
  return fetched(address, (answer) => answer.json() as Promise<ListedRecipe[]>, options);
}

/** The library's recipe `name`. */
export function recipeNamed(name: string, options: On = {}): Promise<KeptRecipe> {
  const path = `/api/v1/pipeline/recipe/${encodeURIComponent(name)}`;
  const address = (options.on ?? server).address(path);
  return fetched(address, (answer) => answer.json() as Promise<KeptRecipe>, options);
}

/** Keeps `recipe` in the library as `name`, faults and all, in place of one of that name. */
export async function saveRecipe(
  name: string,
  recipe: Recipe,
  options: Following = {},
): Promise<void> {
  await act("/api/v1/pipeline/save_recipe", { name, recipe }, options);
}

/** Deletes the library's recipe `name`: a user's. */
export async function deleteRecipe(name: string, options: Following = {}): Promise<void> {
  await act("/api/v1/pipeline/delete_recipe", { name }, options);
}

/**
 * Runs `recipe` on `records` of `project`, or on a region of each in µm: a
 * `workspace.run` job each, in the records' order.
 */
export async function runRecipe(
  project: string,
  recipe: WhichRecipe,
  records: RunRecord[],
  options: Following & { roi?: Schema<"pipeline_run_input">["roi"]; tile?: number } = {},
): Promise<JobRef[]> {
  const { roi, tile, ...following } = options;
  const input = { project, recipe, records, ...(roi ? { roi } : {}), ...(tile ? { tile } : {}) };
  return (await act("/api/v1/pipeline/run", input, following)) as JobRef[];
}

/** What a live recipe was checked to take: the whole, each step, and the budget. */
export type LiveCheck = NonNullable<Schema<"preview_set_live_recipe_output">>;

/** The recipe the preview runs now, if any (P22). */
export function liveRecipe(options: On = {}): Promise<Recipe | null> {
  const address = (options.on ?? server).address("/api/v1/preview/live_recipe");
  return fetched(address, (answer) => answer.json() as Promise<Recipe | null>, options);
}

/**
 * Has the preview run `recipe` on each frame, or none: what it was checked
 * to take. The server refuses one that can't run live, or is over budget.
 */
export async function setLiveRecipe(
  recipe: Recipe | null,
  options: Following = {},
): Promise<LiveCheck | null> {
  const input = recipe ? { recipe } : {};
  return (await act("/api/v1/preview/set_live_recipe", input, options)) ?? null;
}
