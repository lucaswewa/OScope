// The app's keyboard shortcuts (W2.1): each a key, what it does in words,
// and what it runs, listed under `?`. A key pressed while typing in a
// field, or with Ctrl, Alt or the command key, is not a shortcut.
import { readonly, shallowRef } from "vue";

export interface Shortcut {
  /** The key, as `KeyboardEvent.key` names it: `?`, `1`. */
  key: string;
  /** What it does, in words, read when the list is shown. */
  label: () => string;
  run: () => void;
}

const registered = shallowRef<readonly Shortcut[]>([]);

/** The shortcuts, in the order they were added. */
export const shortcuts = readonly(registered);

/** Adds a shortcut, and gives the means to take it away. A key that is taken already is refused. */
export function addShortcut(shortcut: Shortcut): () => void {
  if (registered.value.some((one) => one.key === shortcut.key)) {
    throw new Error(`the key ${shortcut.key} is a shortcut already`);
  }
  registered.value = [...registered.value, shortcut];
  return () => {
    registered.value = registered.value.filter((one) => one !== shortcut);
  };
}

/** Whether a key press is someone typing, or a chord: shortcuts leave it alone. */
export function isTyping(event: KeyboardEvent): boolean {
  if (event.ctrlKey || event.altKey || event.metaKey) return true;
  const target = event.target as HTMLElement | null;
  if (!target?.closest) return false;
  return Boolean(
    target.isContentEditable ||
    target.closest("input, textarea, select, [role='textbox'], [role='combobox'], [role='slider']"),
  );
}

/** Runs the shortcut a key press is, if it is one: whether it was. */
export function press(event: KeyboardEvent): boolean {
  if (event.defaultPrevented || event.repeat || isTyping(event)) return false;
  const shortcut = registered.value.find((one) => one.key === event.key);
  if (!shortcut) return false;
  event.preventDefault();
  shortcut.run();
  return true;
}

/** Listens for shortcuts on `target`; gives the means to stop. */
export function listen(target: EventTarget = window): () => void {
  const pressed = (event: Event) => void press(event as KeyboardEvent);
  target.addEventListener("keydown", pressed);
  return () => target.removeEventListener("keydown", pressed);
}
