// The graph editor's model (W7.2), on the catalogue and the stock recipes
// recorded from a server (scripts/record-pipeline.mjs): the recipe as a
// graph and back, for every stock recipe; the types' rule; each edit; the
// keyboard's path; undo and redo; the palette's search; a recipe's file;
// and the automatic layout.
import { describe, expect, it } from "vitest";

import type { NodeDescriptor, Recipe } from "@/api/pipeline";
import {
  canConnect,
  catalogueOf,
  copied,
  drawn,
  emptyRecipe,
  freeId,
  INPUTS,
  moved,
  NEW_OUTPUT,
  OUTPUTS,
  pasted,
  pressPort,
  recipeOf,
  withEdge,
  withNode,
  withoutEdge,
  withoutNodes,
} from "./graph";
import { KEPT, useHistory } from "./history";
import { laidOut } from "./layout";
import { found } from "./palette";
import { asRecipe, fileOf } from "./recipeFile";
import recorded from "./recorded-pipeline.json";

const nodes = recorded.nodes as unknown as NodeDescriptor[];
const catalogue = catalogueOf(nodes);
const stock = recorded.recipes as unknown as Record<string, Recipe>;
const descriptor = (id: string) => catalogue.get(id)!;
const at = (node: string, port: string) => ({ node, port });

/** A blur, then Otsu's threshold, its mask the recipe's output. */
function blurred(): Recipe {
  let recipe = withNode(emptyRecipe("blurred"), descriptor("filter.gaussian"), [100, 0]);
  recipe = withNode(recipe, descriptor("threshold.otsu"), [300, 0]);
  recipe = withEdge(recipe, at(INPUTS, "image"), at("gaussian", "image"));
  recipe = withEdge(recipe, at("gaussian", "image"), at("otsu", "image"));
  return withEdge(recipe, at("otsu", "mask"), at(OUTPUTS, NEW_OUTPUT));
}

describe("a recipe as a graph", () => {
  it("is drawn and read back without loss, for every stock recipe", () => {
    expect(Object.keys(stock).length).toBeGreaterThanOrEqual(6);
    for (const [name, recipe] of Object.entries(stock)) {
      const placed = laidOut(recipe, catalogue);
      const { nodes: drawnNodes, edges } = drawn(placed, catalogue);
      expect(recipeOf(placed, drawnNodes, edges), name).toEqual(placed);
      // Each edge and each output, drawn once.
      expect(edges.length, name).toBe(recipe.edges.length + recipe.outputs.length);
    }
  });

  it("has the recipe's inputs as a node first, and its outputs as one last", () => {
    const { nodes: drawnNodes, edges } = drawn(blurred(), catalogue);
    expect(drawnNodes.map((node) => [node.id, node.kind])).toEqual([
      [INPUTS, "inputs"],
      ["gaussian", "step"],
      ["otsu", "step"],
      [OUTPUTS, "outputs"],
    ]);
    expect(drawnNodes.at(-1)!.inputs).toEqual([{ name: "mask", type: "mask" }]);
    expect(edges.map((edge) => [edge.id, edge.type])).toEqual([
      ["input.image->gaussian.image", "image"],
      ["gaussian.image->otsu.image", "image"],
      ["otsu.mask->output.mask", "mask"],
    ]);
  });

  it("puts each fault on its node, and its port", () => {
    const faults = [
      { at: "nodes[1] (otsu)", node: "otsu", port: "image", message: "no edge" },
      { at: "nodes[0] (gaussian)", node: "gaussian", message: "a bad sigma" },
      { at: "nodes[0] (gaussian) params/sigma", node: "gaussian", message: "is missing" },
      { at: "outputs", message: "nothing given" },
    ];
    const { nodes: drawnNodes } = drawn(blurred(), catalogue, faults);
    const of = (id: string) => drawnNodes.find((node) => node.id === id)!.faults;
    expect(of("otsu")).toEqual({ image: ["no edge"] });
    expect(of("gaussian")).toEqual({ "": ["a bad sigma", "sigma: is missing"] });
    expect(of(INPUTS)).toEqual({});
  });
});

describe("a connection", () => {
  const recipe = blurred();

  it("joins ports of the same type, and only those", () => {
    expect(canConnect(recipe, catalogue, at("gaussian", "image"), at("otsu", "image"))).toBe(true);
    expect(canConnect(recipe, catalogue, at("otsu", "mask"), at("otsu", "image"))).toBe(false);
    const open = withNode(recipe, descriptor("morphology.open"), [0, 0]);
    expect(canConnect(open, catalogue, at("otsu", "mask"), at("open", "mask"))).toBe(true);
    expect(canConnect(open, catalogue, at("gaussian", "image"), at("open", "mask"))).toBe(false);
  });

  it("never goes into the recipe's inputs, or out of its outputs, or to a port that isn't there", () => {
    expect(canConnect(recipe, catalogue, at("gaussian", "image"), at(INPUTS, "image"))).toBe(false);
    expect(canConnect(recipe, catalogue, at(OUTPUTS, "mask"), at("otsu", "image"))).toBe(false);
    expect(canConnect(recipe, catalogue, at("gaussian", "nothing"), at("otsu", "image"))).toBe(
      false,
    );
  });

  it("makes a new output of any type, and keeps an output's type", () => {
    expect(canConnect(recipe, catalogue, at("gaussian", "image"), at(OUTPUTS, NEW_OUTPUT))).toBe(
      true,
    );
    expect(canConnect(recipe, catalogue, at("gaussian", "image"), at(OUTPUTS, "mask"))).toBe(false);
    expect(canConnect(recipe, catalogue, at("otsu", "mask"), at(OUTPUTS, "mask"))).toBe(true);
  });
});

describe("an edit", () => {
  it("adds a node of a free ID, where it is put, with no parameters", () => {
    const recipe = withNode(blurred(), descriptor("filter.gaussian"), [5, 6]);
    expect(recipe.nodes.at(-1)).toEqual({ id: "gaussian_2", node: "filter.gaussian", params: {} });
    expect(recipe.layout!.gaussian_2).toEqual([5, 6]);
    expect(freeId(recipe, "filter.gaussian")).toBe("gaussian_3");
    expect(freeId(emptyRecipe("x"), "io.input")).toBe("input_2");
  });

  it("joins an input in place of what it took, and names a new output after its port", () => {
    let recipe = withEdge(blurred(), at(INPUTS, "image"), at("otsu", "image"));
    expect(recipe.edges.filter((edge) => edge.to === "otsu.image")).toEqual([
      { from: "input.image", to: "otsu.image" },
    ]);
    recipe = withEdge(recipe, at("otsu", "mask"), at(OUTPUTS, NEW_OUTPUT));
    expect(recipe.outputs.map((output) => output.name)).toEqual(["mask", "mask_2"]);
    recipe = withoutEdge(recipe, at(OUTPUTS, "mask"));
    expect(recipe.outputs).toEqual([{ name: "mask_2", from: "otsu.mask" }]);
    recipe = withoutEdge(recipe, at("otsu", "image"));
    expect(recipe.edges.some((edge) => edge.to === "otsu.image")).toBe(false);
  });

  it("deletes nodes with their edges, outputs and places, never the recipe's ends", () => {
    const recipe = withoutNodes(blurred(), ["gaussian", INPUTS, OUTPUTS]);
    expect(recipe.nodes.map((used) => used.id)).toEqual(["otsu"]);
    expect(recipe.edges).toEqual([]);
    expect(recipe.outputs).toEqual([{ name: "mask", from: "otsu.mask" }]);
    expect(Object.keys(recipe.layout!)).toEqual(["otsu"]);
    // An edge already gone with its node: nothing more to delete.
    expect(withoutEdge(recipe, { node: "otsu", port: "image" })).toBe(recipe);
  });

  it("moves a node, and leaves the recipe as it is when it doesn't move", () => {
    const recipe = blurred();
    expect(moved(recipe, "otsu", [300, 0])).toBe(recipe);
    expect(moved(recipe, "otsu", [310, 20]).layout!.otsu).toEqual([310, 20]);
  });

  it("copies nodes and the edges between them, and pastes them under free IDs, moved", () => {
    const recipe = blurred();
    const copy = copied(recipe, ["gaussian", "otsu"]);
    expect(copy.edges).toEqual([{ from: "gaussian.image", to: "otsu.image" }]);
    const done = pasted(recipe, copy);
    expect(done.ids).toEqual(["gaussian_2", "otsu_2"]);
    expect(done.recipe.edges.at(-1)).toEqual({ from: "gaussian_2.image", to: "otsu_2.image" });
    expect(done.recipe.layout!.gaussian_2).toEqual([140, 40]);
    // The copy's own: changing the pasted leaves the copy as it was.
    expect(copy.nodes[0]!.id).toBe("gaussian");
  });
});

describe("the keyboard's path", () => {
  const recipe = blurred();

  it("starts at an output, and ends at an input of its type", () => {
    const started = pressPort(recipe, catalogue, undefined, at("gaussian", "image"), "output");
    expect(started).toEqual({ linking: at("gaussian", "image"), said: "from" });
    const ended = pressPort(recipe, catalogue, started.linking, at("otsu", "image"), "input");
    expect(ended).toEqual({
      linking: undefined,
      join: { from: at("gaussian", "image"), to: at("otsu", "image") },
      said: "joined",
    });
  });

  it("refuses an input of another type, and asks for an output first", () => {
    const linking = at("otsu", "mask");
    expect(pressPort(recipe, catalogue, linking, at("otsu", "image"), "input")).toEqual({
      linking,
      said: "refused",
    });
    expect(pressPort(recipe, catalogue, undefined, at("otsu", "image"), "input")).toEqual({
      linking: undefined,
      said: "first",
    });
  });
});

describe("undo and redo", () => {
  it("go back and forth through the recipes, an edit cutting off what was undone", () => {
    const first = emptyRecipe("first");
    const history = useHistory(first);
    const second = withNode(first, descriptor("filter.gaussian"), [0, 0]);
    const third = withNode(second, descriptor("threshold.otsu"), [0, 0]);
    history.edit(second);
    history.edit(third);
    history.undo();
    history.undo();
    expect(history.now.value).toBe(first);
    expect(history.canUndo.value).toBe(false);
    history.redo();
    expect(history.now.value).toBe(second);
    history.edit(third);
    expect(history.canRedo.value).toBe(false);
    history.reset(first);
    expect([history.canUndo.value, history.canRedo.value]).toEqual([false, false]);
  });

  it("keep the last hundred, and no edit that changes nothing", () => {
    const history = useHistory(emptyRecipe("0"));
    for (let i = 1; i <= KEPT + 5; i++) history.edit(emptyRecipe(String(i)));
    history.edit(history.now.value);
    let back = 0;
    while (history.canUndo.value) {
      history.undo();
      back++;
    }
    expect(back).toBe(KEPT);
    expect(history.now.value.name).toBe("5");
  });
});

describe("the palette", () => {
  it("finds nodes by every word of their ID and what they do, by category", () => {
    const groups = found(nodes, "GAUSSIAN blur");
    expect(groups).toEqual([{ category: "filter", nodes: [descriptor("filter.gaussian")] }]);
    const all = found(nodes, "");
    const categories = all.map((group) => group.category);
    expect(categories).toEqual([...categories].sort((a, b) => a.localeCompare(b)));
    expect(all.flatMap((group) => group.nodes).length).toBe(nodes.length);
    expect(found(nodes, "no node does this")).toEqual([]);
  });
});

describe("a recipe's file", () => {
  it("reads as a recipe, what may be left out filled in", () => {
    const json = { format: 1, nodes: [], outputs: [] };
    expect(asRecipe(json)).toEqual({
      format: 1,
      name: "",
      inputs: [],
      nodes: [],
      edges: [],
      outputs: [],
      layout: {},
    });
    expect(asRecipe(JSON.parse(JSON.stringify(stock.levels)))).toEqual({
      layout: {},
      ...stock.levels,
    });
  });

  it("is written in one order, whatever order the recipe has, and read back as it was", () => {
    const { outputs, edges, ...rest } = blurred();
    const text = fileOf({ edges, ...rest, outputs });
    expect(Object.keys(JSON.parse(text))).toEqual([
      "format",
      "name",
      "inputs",
      "nodes",
      "edges",
      "outputs",
      "layout",
    ]);
    expect(fileOf(asRecipe(JSON.parse(text)))).toBe(text);
  });

  it("is refused, saying why, when it isn't a recipe", () => {
    expect(() => asRecipe([])).toThrow("not a JSON object");
    expect(() => asRecipe({ nodes: [], outputs: [] })).toThrow("no format");
    expect(() => asRecipe({ format: 1, outputs: [] })).toThrow("no nodes");
    expect(() => asRecipe({ format: 1, nodes: [], outputs: [], edges: {} })).toThrow(
      "edges aren't a list",
    );
    expect(() => asRecipe({ format: 1, nodes: [], outputs: [], layout: { a: [1] } })).toThrow(
      "layout's a isn't a place",
    );
  });
});

describe("the automatic layout", () => {
  it("puts the recipe's inputs first, each node after what it reads, and its outputs last", () => {
    const placed = laidOut(blurred(), catalogue).layout!;
    const x = (id: string) => placed[id]![0]!;
    expect(x(INPUTS)).toBeLessThan(x("gaussian"));
    expect(x("gaussian")).toBeLessThan(x("otsu"));
    expect(x("otsu")).toBeLessThan(x(OUTPUTS));
  });
});
