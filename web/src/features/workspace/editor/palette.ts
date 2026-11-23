// The palette's search (W7.2): the nodes whose ID or description holds
// every word typed, in any case, by category, each category's in order.
import type { NodeDescriptor } from "@/api/pipeline";

/** The nodes `text` finds, grouped by category, the categories in order. */
export function found(
  nodes: readonly NodeDescriptor[],
  text: string,
): { category: string; nodes: NodeDescriptor[] }[] {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const matches = (node: NodeDescriptor) => {
    const said = `${node.id} ${node.description}`.toLowerCase();
    return words.every((word) => said.includes(word));
  };
  const groups = new Map<string, NodeDescriptor[]>();
  for (const node of nodes.filter(matches)) {
    const list = groups.get(node.category) ?? [];
    list.push(node);
    groups.set(node.category, list);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([category, list]) => ({
      category,
      nodes: list.sort((a, b) => a.id.localeCompare(b.id)),
    }));
}
