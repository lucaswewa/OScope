// What differs from the recipe saved (W7.3): each node's parameters as they
// take effect, its defaults under what it sets, so a parameter set to its
// default is no change; and whether anything differs that saving would keep.
import type { NodeDescriptor, Recipe } from "@/api/pipeline";
import type { Catalogue } from "@/features/workspace/editor/graph";
import { shapeOf, type Json } from "@/ui/schema-form/shape";

type Params = Record<string, Json>;
const isRecord = (value: unknown): value is Params =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** A node's parameters' defaults, those its schema gives. */
export function defaultsOf(descriptor: NodeDescriptor | undefined): Params {
  const shape = descriptor ? shapeOf(descriptor.params) : undefined;
  if (shape?.kind !== "object") return {};
  const given = shape.fields.filter((field) => field.shape.default !== undefined);
  return Object.fromEntries(
    given.map((field) => [field.name, structuredClone(field.shape.default!)]),
  );
}

/** A node's parameters as they take effect: its defaults, then what it sets. */
export function paramsOf(used: Recipe["nodes"][number], catalogue: Catalogue): Params {
  return { ...defaultsOf(catalogue.get(used.node)), ...(isRecord(used.params) ? used.params : {}) };
}

/** JSON with every object's keys in order: two values that mean the same, written the same. */
function canonical(value: unknown): string {
  return JSON.stringify(value ?? null, (_, one: unknown) =>
    isRecord(one)
      ? Object.fromEntries(Object.entries(one).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : one,
  );
}

export interface Changes {
  /** Whether anything differs that saving would keep: places too. */
  unsaved: boolean;
  /** Each node's parameters that differ, by node ID: one added, every one it has. */
  params: Record<string, string[]>;
}

/** What differs in `now` from `saved`. */
export function changesOf(saved: Recipe, now: Recipe, catalogue: Catalogue): Changes {
  const before = new Map(saved.nodes.map((used) => [used.id, used]));
  const params: Record<string, string[]> = {};
  for (const used of now.nodes) {
    const was = before.get(used.id);
    const then = was?.node === used.node ? paramsOf(was, catalogue) : {};
    const is = paramsOf(used, catalogue);
    const names = new Set([...Object.keys(then), ...Object.keys(is)]);
    const differ = [...names].filter((name) => canonical(then[name]) !== canonical(is[name]));
    if (differ.length) params[used.id] = differ.sort();
  }
  const effective = (recipe: Recipe) =>
    canonical({
      ...recipe,
      nodes: recipe.nodes.map((used) => ({ ...used, params: paramsOf(used, catalogue) })),
    });
  return { unsaved: effective(saved) !== effective(now), params };
}
