// The editor's parameters, library and costs (W7.3), on the catalogue and
// the stock recipes recorded from a server: what counts as changed since a
// recipe was saved; a node's parameters with its defaults; the library's
// names; and the costs as they are shown.
import { describe, expect, it } from "vitest";

import type { ListedRecipe, NodeDescriptor, Recipe } from "@/api/pipeline";

import { changesOf, defaultsOf, paramsOf } from "./changes";
import { costOf, costsOf } from "./costs";
import { catalogueOf, emptyRecipe, moved, withNode, withParams } from "./graph";
import { nameFor, nameProblem, replaces } from "./library";
import recorded from "./recorded-pipeline.json";

const catalogue = catalogueOf(recorded.nodes as unknown as NodeDescriptor[]);
const stock = recorded.recipes as unknown as Record<string, Recipe>;
const levels = stock.levels!;
const node = (recipe: Recipe, id: string) => recipe.nodes.find((used) => used.id === id)!;

describe("a node's parameters", () => {
  it("are its defaults, under what it sets", () => {
    expect(defaultsOf(catalogue.get("threshold.otsu"))).toEqual({ bins: 256 });
    expect(defaultsOf(catalogue.get("filter.gaussian"))).toEqual({});
    expect(defaultsOf(undefined)).toEqual({});
    const otsu = { id: "otsu", node: "threshold.otsu", params: {} };
    expect(paramsOf(otsu, catalogue)).toEqual({ bins: 256 });
    expect(paramsOf({ ...otsu, params: { bins: 64 } }, catalogue)).toEqual({ bins: 64 });
  });
});

describe("what differs from the recipe saved", () => {
  it("is nothing, for the recipe as it was", () => {
    expect(changesOf(levels, structuredClone(levels), catalogue)).toEqual({
      unsaved: false,
      params: {},
    });
  });

  it("is each parameter changed, by node", () => {
    const params = { ...(node(levels, "levels").params as object), white: 0.5 };
    const changes = changesOf(levels, withParams(levels, "levels", params), catalogue);
    expect(changes).toEqual({ unsaved: true, params: { levels: ["white"] } });
  });

  it("isn't a parameter set to its default, or the order of its keys", () => {
    const otsu = withNode(emptyRecipe("otsu"), catalogue.get("threshold.otsu")!, [0, 0]);
    const set = withParams(otsu, "otsu", { bins: 256 });
    expect(changesOf(otsu, set, catalogue)).toEqual({ unsaved: false, params: {} });
    const { black, white, gamma } = node(levels, "levels").params as Record<string, number>;
    const reordered = withParams(levels, "levels", { gamma, white, black });
    expect(changesOf(levels, reordered, catalogue).unsaved).toBe(false);
    const flipped = Object.fromEntries(Object.entries(levels).reverse()) as Recipe;
    expect(changesOf(levels, flipped, catalogue).unsaved).toBe(false);
  });

  it("is every parameter of a node added, and a node moved, or the name, unsaved", () => {
    const added = withNode(levels, catalogue.get("threshold.otsu")!, [0, 0]);
    expect(changesOf(levels, added, catalogue)).toEqual({
      unsaved: true,
      params: { otsu: ["bins"] },
    });
    const placed = withNode(levels, catalogue.get("threshold.otsu")!, [0, 0]);
    expect(changesOf(placed, moved(placed, "otsu", [10, 0]), catalogue)).toEqual({
      unsaved: true,
      params: {},
    });
    expect(changesOf(levels, { ...levels, name: "other" }, catalogue).unsaved).toBe(true);
  });
});

describe("the library's names", () => {
  const listed: ListedRecipe[] = [
    { name: "levels", title: "Levels and gamma", stock: true },
    { name: "mine", title: "Mine", stock: false },
  ];

  it("are 1 to 64 of a–z, 0–9, - and _, and not a stock recipe's", () => {
    expect(nameProblem("my-levels_2", listed)).toBeUndefined();
    expect(nameProblem("", listed)).toBe("empty");
    expect(nameProblem("a".repeat(65), listed)).toBe("long");
    expect(nameProblem("My levels", listed)).toBe("characters");
    expect(nameProblem("../up", listed)).toBe("characters");
    expect(nameProblem("levels", listed)).toBe("stock");
  });

  it("replace a user's recipe of that name, saying so", () => {
    expect(replaces("mine", listed)).toBe(true);
    expect(replaces("levels", listed)).toBe(false);
    expect(replaces("new", listed)).toBe(false);
  });

  it("are suggested from a recipe's name, in words joined by -", () => {
    expect(nameFor("Levels and gamma")).toBe("levels-and-gamma");
    expect(nameFor("  Zellkern Zählung! ")).toBe("zellkern-zahlung");
    expect(nameFor("x".repeat(80))).toHaveLength(64);
  });
});

describe("the costs", () => {
  it("are shown in tenths of a ms below 10, in whole ms from there, against the budget", () => {
    expect(costOf(1.234, 20)).toEqual({ ms: "1.2", percent: 6, over: false });
    expect(costOf(12.6, 20)).toEqual({ ms: "13", percent: 63, over: false });
    expect(costOf(25, 20)).toEqual({ ms: "25", percent: 125, over: true });
  });

  it("are each step's, by its ID", () => {
    const costs = costsOf({ levels: 4, invert: 1 }, 10);
    expect([...costs.keys()]).toEqual(["levels", "invert"]);
    expect(costs.get("levels")).toEqual({ ms: "4.0", percent: 40, over: false });
  });
});
