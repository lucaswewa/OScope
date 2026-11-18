// What a tree holds (ADR-0042).
import type { IconName } from "./icons";

/** An item of a tree, and the items under it. */
export interface Branch {
  id: string;
  label: string;
  icon?: IconName;
  children?: Branch[];
}

/** The item with `id`, wherever it is in the tree. */
export function findBranch(items: readonly Branch[], id: string | undefined): Branch | undefined {
  if (id === undefined) return undefined;
  for (const item of items) {
    if (item.id === id) return item;
    const below = item.children && findBranch(item.children, id);
    if (below) return below;
  }
  return undefined;
}
