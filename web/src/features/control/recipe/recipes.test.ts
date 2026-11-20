import { describe, expect, it } from "vitest";

import { needsOf, recipeFrom, STOCK, type Recipe } from "./recipes";

/** A recipe's faults of wiring: each edge's ends, and each output's, found. */
function faultsOf(recipe: Recipe): string[] {
  const ids = new Set(recipe.nodes.map((node) => node.id));
  const inputs = new Set(recipe.inputs.map((input) => `input.${input.name}`));
  const isPort = (end: string) => inputs.has(end) || ids.has(end.split(".")[0]!);
  return [
    ...(ids.size === recipe.nodes.length ? [] : ["an ID twice"]),
    ...recipe.edges.flatMap(({ from, to }) =>
      isPort(from) && ids.has(to.split(".")[0]!) ? [] : [`${from} → ${to}`],
    ),
    ...recipe.outputs.flatMap(({ from }) => (isPort(from) ? [] : [`output ${from}`])),
  ];
}

describe("the recipes that come with the app", () => {
  it("are each wired whole, from the image to an image", () => {
    for (const { key, recipe } of STOCK) {
      expect(faultsOf(recipe), key).toEqual([]);
      expect(recipe.format, key).toBe(1);
      expect(recipe.inputs[0], key).toEqual({ name: "image", type: "image" });
      expect(recipe.outputs, key).toEqual([
        { name: "image", from: expect.stringMatching(/\.image$/) },
      ]);
    }
  });

  it("run their steps in order, each on the last one's image", () => {
    const inverted = STOCK.find((one) => one.key === "inverted")!.recipe;
    expect(inverted.edges).toEqual([
      { from: "input.image", to: "levels.image" },
      { from: "levels.image", to: "invert.image" },
    ]);
    expect(inverted.outputs[0]!.from).toBe("invert.image");
  });

  it("say which references they read, each once", () => {
    const flat = STOCK.find((one) => one.key === "flat")!.recipe;
    expect(needsOf(flat)).toEqual(["flat"]);
    expect(flat.edges).toContainEqual({ from: "flat.image", to: "even.flat" });
    expect(flat.edges).toContainEqual({ from: "input.region", to: "even.region" });
    expect(needsOf(STOCK.find((one) => one.key === "inverted")!.recipe)).toEqual([]);
    const twice = {
      ...flat,
      nodes: [...flat.nodes, { id: "again", node: "io.reference", params: { name: "flat" } }],
    };
    expect(needsOf(twice)).toEqual(["flat"]);
  });
});

describe("a recipe opened from a file", () => {
  it("is taken as it is written, its lists' defaults filled in", () => {
    const flat = STOCK.find((one) => one.key === "flat")!.recipe;
    expect(recipeFrom(JSON.stringify(flat))).toEqual(flat);
    const bare = recipeFrom(
      '{"format": 1, "nodes": [{"id": "l", "node": "display.levels"}], "outputs": [{"name": "image", "from": "l.image"}]}',
    );
    expect([bare.name, bare.inputs, bare.edges]).toEqual(["Opened recipe", [], []]);
  });

  it("is refused, saying why, when it isn't a recipe", () => {
    expect(() => recipeFrom("{format: 1")).toThrow("it isn't JSON");
    expect(() => recipeFrom("[]")).toThrow("a recipe is an object");
    expect(() => recipeFrom('{"format": 2}')).toThrow("its format isn't 1");
    expect(() => recipeFrom('{"format": 1, "nodes": []}')).toThrow("it has no nodes");
    expect(() => recipeFrom('{"format": 1, "nodes": [{"id": "l", "node": "x"}]}')).toThrow(
      "it has no outputs",
    );
  });
});
