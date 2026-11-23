// The editor's undo and redo (W7.2): each recipe the graph has been, the
// newest last, a hundred at most. An edit cuts off whatever was undone.
import { computed, shallowRef } from "vue";

import type { Recipe } from "@/api/pipeline";

/** The most recipes kept to go back to. */
export const KEPT = 100;

/** A history of recipes, from `first`. */
export function useHistory(first: Recipe) {
  const past = shallowRef<Recipe[]>([]);
  const now = shallowRef<Recipe>(first);
  const future = shallowRef<Recipe[]>([]);
  return {
    /** The recipe as it is. */
    now: computed(() => now.value),
    canUndo: computed(() => past.value.length > 0),
    canRedo: computed(() => future.value.length > 0),
    /** Makes `next` the recipe, the one before it kept to go back to. */
    edit(next: Recipe): void {
      if (next === now.value) return;
      past.value = [...past.value, now.value].slice(-KEPT);
      now.value = next;
      future.value = [];
    },
    undo(): void {
      const previous = past.value.at(-1);
      if (!previous) return;
      past.value = past.value.slice(0, -1);
      future.value = [now.value, ...future.value];
      now.value = previous;
    },
    redo(): void {
      const next = future.value[0];
      if (!next) return;
      future.value = future.value.slice(1);
      past.value = [...past.value, now.value];
      now.value = next;
    },
    /** Starts again at `recipe`, nothing to undo: one opened, or made new. */
    reset(recipe: Recipe): void {
      past.value = [];
      future.value = [];
      now.value = recipe;
    },
  };
}

export type History = ReturnType<typeof useHistory>;
