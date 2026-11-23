// What the editor's parts share (W7.2): the connection being made from the
// keyboard, and its ports' presses; and `check`, run as the recipe changes,
// its faults kept until the next answer.
import { onScopeDispose, shallowRef, watch, type InjectionKey, type Ref } from "vue";

import { ApiError } from "@/api/errors";
import { checkRecipe, type Fault, type Recipe } from "@/api/pipeline";
import type { End, Linking } from "@/features/workspace/editor/graph";

/** What a node of the canvas asks of the editor. */
export interface EditorContext {
  /** The output a connection is being made from, from the keyboard. */
  linking: Ref<Linking>;
  /** A port pressed: an output starts a connection, an input ends one. */
  press(end: End, side: "input" | "output"): void;
}

export const EDITOR: InjectionKey<EditorContext> = Symbol("editor");

/** How long the recipe is left still before it is checked, in ms. */
export const CHECK_AFTER = 300;

/** `check`, run on `recipe` once it has been still a moment: its faults, and why it couldn't be checked. */
export function useChecking(recipe: Ref<Recipe>) {
  const faults = shallowRef<Fault[]>([]);
  const checking = shallowRef(false);
  const unchecked = shallowRef<string>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let asked: AbortController | undefined;
  async function check(): Promise<void> {
    asked?.abort();
    const mine = (asked = new AbortController());
    checking.value = true;
    try {
      const found = await checkRecipe(recipe.value, { signal: mine.signal });
      if (mine.signal.aborted) return;
      faults.value = found;
      unchecked.value = undefined;
    } catch (thrown) {
      if (mine.signal.aborted) return;
      unchecked.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    } finally {
      if (!mine.signal.aborted) checking.value = false;
    }
  }
  watch(
    recipe,
    () => {
      clearTimeout(timer);
      timer = setTimeout(() => void check(), CHECK_AFTER);
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    clearTimeout(timer);
    asked?.abort();
  });
  return { faults, checking, unchecked };
}
