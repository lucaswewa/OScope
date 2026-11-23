// The graph editor's model (W7.2, ADR-0053): a recipe (ADR-0017) is the
// graph. Its nodes are the graph's, placed by the recipe's `layout`; its own
// inputs are one node, `input`, and its outputs another, `output`. Every
// edit is a function from a recipe to the next, so that undo and redo keep
// recipes, and the canvas draws whatever recipe it is given.
import type { Fault, NodeDescriptor, Recipe } from "@/api/pipeline";

/** The recipe's own inputs, as a node: `input.image` in its edges. */
export const INPUTS = "input";
/** The recipe's outputs, as a node: each output an input of it. */
export const OUTPUTS = "output";
/** The outputs' port that makes a new output, named after what it is given. */
export const NEW_OUTPUT = "+";

export type Catalogue = ReadonlyMap<string, NodeDescriptor>;
type Port = { name: string; type: string; optional?: boolean };

/** The catalogue by node ID. */
export function catalogueOf(nodes: readonly NodeDescriptor[]): Catalogue {
  return new Map(nodes.map((node) => [node.id, node]));
}

/** A port of the graph: a node's, by its ID in the recipe, and the port's name. */
export interface End {
  node: string;
  port: string;
}

export const endOf = (reference: string): End => {
  const dot = reference.indexOf(".");
  return { node: reference.slice(0, dot), port: reference.slice(dot + 1) };
};
export const referenceOf = (end: End) => `${end.node}.${end.port}`;

/** A node of the graph, as the canvas draws it. */
export interface DrawnNode {
  id: string;
  kind: "step" | "inputs" | "outputs";
  /** The node's ID in the catalogue, for a step. */
  node?: string;
  title: string;
  x: number;
  y: number;
  inputs: Port[];
  outputs: Port[];
  /** The faults at the node, and at each of its ports, by name: `""` for the node's own. */
  faults: Record<string, string[]>;
}

/** An edge of the graph, as the canvas draws it. */
export interface DrawnEdge {
  id: string;
  from: End;
  to: End;
  type: string;
}

/** The type of a port: a step's, from the catalogue; a recipe input's, as declared. */
function outputsOf(recipe: Recipe, catalogue: Catalogue, id: string): Port[] {
  if (id === INPUTS) return recipe.inputs.map((port) => ({ name: port.name, type: port.type }));
  const used = recipe.nodes.find((node) => node.id === id);
  const descriptor = used && catalogue.get(used.node);
  return (descriptor?.outputs ?? []).map((port) => ({ name: port.name, type: port.type }));
}

function inputsOf(recipe: Recipe, catalogue: Catalogue, id: string): Port[] {
  if (id === OUTPUTS) {
    return recipe.outputs.map((output) => ({
      name: output.name,
      type: typeOf(recipe, catalogue, endOf(output.from)) ?? "",
    }));
  }
  const used = recipe.nodes.find((node) => node.id === id);
  const descriptor = used && catalogue.get(used.node);
  return (descriptor?.inputs ?? []).map((port) => ({
    name: port.name,
    type: port.type,
    optional: port.optional ?? false,
  }));
}

/** The type an output end gives, if it is there. */
export function typeOf(recipe: Recipe, catalogue: Catalogue, end: End): string | undefined {
  return outputsOf(recipe, catalogue, end.node).find((port) => port.name === end.port)?.type;
}

/** Where the recipe's `layout` puts node `id`, or `otherwise`: a place is a pair. */
function placeOf(recipe: Recipe, id: string, otherwise: [number, number]): [number, number] {
  const [x, y] = recipe.layout?.[id] ?? [];
  return x === undefined || y === undefined ? otherwise : [x, y];
}

/** A fault as its node says it: a parameter's names the parameter, by its JSON Pointer. */
function saidOf(fault: Fault): string {
  const pointer = / params\/(.+)$/.exec(fault.at)?.[1];
  return pointer ? `${pointer}: ${fault.message}` : fault.message;
}

/** The recipe as the canvas draws it: its nodes, the two ends among them, and its edges. */
export function drawn(
  recipe: Recipe,
  catalogue: Catalogue,
  faults: readonly Fault[] = [],
): { nodes: DrawnNode[]; edges: DrawnEdge[] } {
  const faultsAt = (id: string) => {
    const at: Record<string, string[]> = {};
    for (const fault of faults) {
      if (fault.node !== id) continue;
      (at[fault.port ?? ""] ??= []).push(saidOf(fault));
    }
    return at;
  };
  const steps = recipe.nodes.map((used, i): DrawnNode => {
    const [x, y] = placeOf(recipe, used.id, [240 + 240 * i, 80]);
    return {
      id: used.id,
      kind: "step",
      node: used.node,
      title: used.node,
      x,
      y,
      inputs: inputsOf(recipe, catalogue, used.id),
      outputs: outputsOf(recipe, catalogue, used.id),
      faults: faultsAt(used.id),
    };
  });
  const [ix, iy] = placeOf(recipe, INPUTS, [0, 80]);
  const [ox, oy] = placeOf(recipe, OUTPUTS, [240 + 240 * recipe.nodes.length, 80]);
  const ends: DrawnNode[] = [
    {
      id: INPUTS,
      kind: "inputs",
      title: INPUTS,
      x: ix,
      y: iy,
      inputs: [],
      outputs: outputsOf(recipe, catalogue, INPUTS),
      faults: faultsAt(INPUTS),
    },
    {
      id: OUTPUTS,
      kind: "outputs",
      title: OUTPUTS,
      x: ox,
      y: oy,
      inputs: inputsOf(recipe, catalogue, OUTPUTS),
      outputs: [],
      faults: {},
    },
  ];
  const edges: DrawnEdge[] = [
    ...recipe.edges.map((edge) => ({
      id: `${edge.from}->${edge.to}`,
      from: endOf(edge.from),
      to: endOf(edge.to),
      type: typeOf(recipe, catalogue, endOf(edge.from)) ?? "",
    })),
    ...recipe.outputs.map((output) => ({
      id: `${output.from}->${OUTPUTS}.${output.name}`,
      from: endOf(output.from),
      to: { node: OUTPUTS, port: output.name },
      type: typeOf(recipe, catalogue, endOf(output.from)) ?? "",
    })),
  ];
  return { nodes: [ends[0]!, ...steps, ends[1]!], edges };
}

/**
 * The recipe the canvas shows, read back from what it draws: its nodes'
 * places from where they are, and its edges and outputs from its edges.
 * The steps' nodes and parameters, its name and its inputs are `base`'s.
 */
export function recipeOf(
  base: Recipe,
  nodes: readonly Pick<DrawnNode, "id" | "x" | "y">[],
  edges: readonly Pick<DrawnEdge, "from" | "to">[],
): Recipe {
  const layout: Record<string, [number, number]> = {};
  for (const node of nodes) layout[node.id] = [node.x, node.y];
  return {
    ...base,
    edges: edges
      .filter((edge) => edge.to.node !== OUTPUTS)
      .map((edge) => ({ from: referenceOf(edge.from), to: referenceOf(edge.to) })),
    outputs: edges
      .filter((edge) => edge.to.node === OUTPUTS)
      .map((edge) => ({ name: edge.to.port, from: referenceOf(edge.from) })),
    layout,
  };
}

/** Whether `from`, an output, may be joined to `to`, an input: the same type, and not the recipe's own end. */
export function canConnect(recipe: Recipe, catalogue: Catalogue, from: End, to: End): boolean {
  if (from.node === to.node || to.node === INPUTS || from.node === OUTPUTS) return false;
  const gives = typeOf(recipe, catalogue, from);
  if (gives === undefined) return false;
  // A new output takes any type; an output kept, the type it has.
  if (to.node === OUTPUTS) {
    const kept = recipe.outputs.find((output) => output.name === to.port);
    return !kept || typeOf(recipe, catalogue, endOf(kept.from)) === gives;
  }
  const takes = inputsOf(recipe, catalogue, to.node).find((port) => port.name === to.port);
  return takes !== undefined && takes.type === gives;
}

/** An ID for a new node of `node`, `blur`, `blur_2`, as no other has. */
export function freeId(recipe: Recipe, node: string): string {
  const stem = (node.split(".").at(-1) ?? "node").replace(/[^A-Za-z0-9_]/g, "_") || "node";
  const taken = new Set([INPUTS, OUTPUTS, ...recipe.nodes.map((used) => used.id)]);
  if (!taken.has(stem)) return stem;
  let n = 2;
  while (taken.has(`${stem}_${n}`)) n++;
  return `${stem}_${n}`;
}

/** An output's name for `port`, as no other output has. */
export function freeOutput(recipe: Recipe, port: string): string {
  const taken = new Set(recipe.outputs.map((output) => output.name));
  if (!taken.has(port)) return port;
  let n = 2;
  while (taken.has(`${port}_${n}`)) n++;
  return `${port}_${n}`;
}

/** `recipe` with a node of `descriptor`, at `[x, y]`, its parameters as the schema's defaults leave them. */
export function withNode(recipe: Recipe, descriptor: NodeDescriptor, at: [number, number]): Recipe {
  const id = freeId(recipe, descriptor.id);
  return {
    ...recipe,
    nodes: [...recipe.nodes, { id, node: descriptor.id, params: {} }],
    layout: { ...recipe.layout, [id]: at },
  };
}

/** `recipe` with `from` joined to `to`: in place of what `to` took, or as an output, named after its port, at the outputs' end. */
export function withEdge(recipe: Recipe, from: End, to: End): Recipe {
  if (to.node === OUTPUTS) {
    const name = to.port === NEW_OUTPUT ? freeOutput(recipe, from.port) : to.port;
    const others = recipe.outputs.filter((output) => output.name !== name);
    return { ...recipe, outputs: [...others, { name, from: referenceOf(from) }] };
  }
  const target = referenceOf(to);
  const others = recipe.edges.filter((edge) => edge.to !== target);
  return { ...recipe, edges: [...others, { from: referenceOf(from), to: target }] };
}

/** `recipe` without the edge into `to`: the same recipe, if it has none. */
export function withoutEdge(recipe: Recipe, to: End): Recipe {
  if (to.node === OUTPUTS) {
    const outputs = recipe.outputs.filter((output) => output.name !== to.port);
    return outputs.length === recipe.outputs.length ? recipe : { ...recipe, outputs };
  }
  const target = referenceOf(to);
  const edges = recipe.edges.filter((edge) => edge.to !== target);
  return edges.length === recipe.edges.length ? recipe : { ...recipe, edges };
}

/** `recipe` without nodes `ids`, their edges and the outputs they gave: the two ends stay. */
export function withoutNodes(recipe: Recipe, ids: readonly string[]): Recipe {
  const gone = new Set(ids.filter((id) => id !== INPUTS && id !== OUTPUTS));
  const touches = (reference: string) => gone.has(endOf(reference).node);
  const layout = Object.fromEntries(
    Object.entries(recipe.layout ?? {}).filter(([id]) => !gone.has(id)),
  );
  return {
    ...recipe,
    nodes: recipe.nodes.filter((used) => !gone.has(used.id)),
    edges: recipe.edges.filter((edge) => !touches(edge.from) && !touches(edge.to)),
    outputs: recipe.outputs.filter((output) => !touches(output.from)),
    layout,
  };
}

/** `recipe` with node `id` at `[x, y]`. */
export function moved(recipe: Recipe, id: string, at: [number, number]): Recipe {
  const was = recipe.layout?.[id];
  if (was && was[0] === at[0] && was[1] === at[1]) return recipe;
  return { ...recipe, layout: { ...recipe.layout, [id]: at } };
}

/** What is copied: nodes, their parameters and places, and the edges between them. */
export interface Copied {
  nodes: Recipe["nodes"];
  edges: Recipe["edges"];
  layout: Record<string, [number, number]>;
}

/** Nodes `ids` of `recipe`, and the edges between them, to paste. */
export function copied(recipe: Recipe, ids: readonly string[]): Copied {
  const kept = new Set(ids);
  const nodes = recipe.nodes.filter((used) => kept.has(used.id));
  const inside = (reference: string) => kept.has(endOf(reference).node);
  const layout: Record<string, [number, number]> = {};
  for (const used of nodes) {
    if (recipe.layout?.[used.id]) layout[used.id] = placeOf(recipe, used.id, [0, 0]);
  }
  return {
    nodes: structuredClone(nodes),
    edges: recipe.edges.filter((edge) => inside(edge.from) && inside(edge.to)),
    layout,
  };
}

/** `recipe` with `copy` pasted, each node given a free ID and moved by `offset`; and the new IDs. */
export function pasted(
  recipe: Recipe,
  copy: Copied,
  offset: [number, number] = [40, 40],
): { recipe: Recipe; ids: string[] } {
  let next = recipe;
  const renamed = new Map<string, string>();
  for (const used of copy.nodes) {
    const id = freeId(next, used.node);
    renamed.set(used.id, id);
    const [x, y] = copy.layout[used.id] ?? [0, 0];
    next = {
      ...next,
      nodes: [...next.nodes, { ...structuredClone(used), id }],
      layout: { ...next.layout, [id]: [x + offset[0], y + offset[1]] },
    };
  }
  const rename = (reference: string) => {
    const end = endOf(reference);
    return referenceOf({ node: renamed.get(end.node) ?? end.node, port: end.port });
  };
  const edges = copy.edges.map((edge) => ({ from: rename(edge.from), to: rename(edge.to) }));
  return { recipe: { ...next, edges: [...next.edges, ...edges] }, ids: [...renamed.values()] };
}

/** An empty recipe: one image in, nothing yet out. */
export function emptyRecipe(name: string): Recipe {
  return {
    format: 1,
    name,
    inputs: [{ name: "image", type: "image" }],
    nodes: [],
    edges: [],
    outputs: [],
    layout: {},
  };
}

/** A connection being made from the keyboard: from an output, until an input is chosen. */
export type Linking = End | undefined;

/** What pressing a port does, from the keyboard. */
export interface Pressed {
  /** The connection being made after it. */
  linking: Linking;
  /** An edge to make. */
  join?: { from: End; to: End };
  /** What happened, for the page to say: a key of the editor's words. */
  said: "from" | "joined" | "refused" | "first";
}

/**
 * A port pressed from the keyboard: an output starts a connection; an input
 * ends one, if the two may be joined; an input with none started asks for
 * an output first.
 */
export function pressPort(
  recipe: Recipe,
  catalogue: Catalogue,
  linking: Linking,
  end: End,
  side: "input" | "output",
): Pressed {
  if (side === "output") return { linking: end, said: "from" };
  if (!linking) return { linking, said: "first" };
  if (!canConnect(recipe, catalogue, linking, end)) return { linking, said: "refused" };
  return { linking: undefined, join: { from: linking, to: end }, said: "joined" };
}
