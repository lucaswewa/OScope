// A value kept between visits, a pane's size, a column's width, by the host:
// the browser's storage, or the desktop app's (ADR-0046).
import { ref, watch, type Ref } from "vue";

import { host } from "./host";

/**
 * The value kept under `key`: what the host kept, as far as `valid` takes
 * it, or `fallback`. Each change to it is kept, whatever is changed in it.
 */
export function kept<T>(key: string, fallback: T, valid: (value: unknown) => value is T): Ref<T> {
  let saved: unknown;
  try {
    saved = JSON.parse(host().kept.read(key) ?? "null");
  } catch {
    saved = null;
  }
  const value = ref(valid(saved) ? saved : fallback) as Ref<T>;
  watch(value, (now) => host().kept.write(key, JSON.stringify(now)), { deep: true });
  return value;
}
