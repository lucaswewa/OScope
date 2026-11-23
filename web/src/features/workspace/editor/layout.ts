// The editor's automatic layout (W7.2): the graph laid out from left to
// right by its edges, as Sugiyama, Tagawa and Toda (1981) lay out a
// directed graph in layers, through dagre: the recipe's inputs first, its
// outputs last.
import { Graph, layout } from "@dagrejs/dagre";

import type { Recipe } from "@/api/pipeline";
import { drawn, type Catalogue } from "@/features/workspace/editor/graph";

/** A node's width, and a port row's height, as the canvas draws them, near enough. */
const WIDTH = 200;
const ROW = 24;

/** `recipe` with every node placed in layers, from its inputs to its outputs. */
export function laidOut(recipe: Recipe, catalogue: Catalogue): Recipe {
  const graph = new Graph();
  graph.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 80, marginx: 20, marginy: 20 });
  graph.setDefaultEdgeLabel(() => ({}));
  const { nodes, edges } = drawn(recipe, catalogue);
  for (const node of nodes) {
    const rows = Math.max(node.inputs.length, node.outputs.length, 1);
    graph.setNode(node.id, { width: WIDTH, height: 48 + ROW * rows });
  }
  for (const edge of edges) graph.setEdge(edge.from.node, edge.to.node);
  layout(graph);
  const placed: Record<string, [number, number]> = {};
  for (const node of nodes) {
    const at = graph.node(node.id);
    placed[node.id] = [Math.round(at.x - at.width / 2), Math.round(at.y - at.height / 2)];
  }
  return { ...recipe, layout: placed };
}
