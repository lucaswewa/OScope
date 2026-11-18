// A value the browser keeps between visits: a pane's size, a column's width.
// The desktop app's host keeps it its own way (W1.4).
import { ref, watch, type Ref } from "vue";

import { read, write } from "./storage";

/**
 * The value kept under `key`: what the browser kept, as far as `valid` takes
 * it, or `fallback`. Each change to it is kept, whatever is changed in it.
 */
export function kept<T>(key: string, fallback: T, valid: (value: unknown) => value is T): Ref<T> {
  let saved: unknown;
  try {
    saved = JSON.parse(read(key) ?? "null");
  } catch {
    saved = null;
  }
  const value = ref(valid(saved) ? saved : fallback) as Ref<T>;
  watch(value, (now) => write(key, JSON.stringify(now)), { deep: true });
  return value;
}
