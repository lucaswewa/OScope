// A scan's positions (W5.4): places visited in turn, each added from where
// the stage is, with its z, where its focus was set by hand; named,
// reordered and removed; as the server's positions take them.
import type { Point } from "@/ui/tile-viewer/coordinates";

import type { ScanRequest, Stack } from "./plan";

/** A place to visit, its own z if it has one, and its name. */
export interface Place extends Point {
  z?: number | undefined;
  name: string;
}

/** `list` with `at` added last, named `nameOf(n)` for the first n no other has. */
export function added(
  list: readonly Place[],
  at: Point & { z?: number | undefined },
  nameOf: (n: number) => string,
): Place[] {
  const taken = new Set(list.map((one) => one.name));
  let n = list.length + 1;
  while (taken.has(nameOf(n))) n += 1;
  return [
    ...list,
    { x: at.x, y: at.y, ...(at.z === undefined ? {} : { z: at.z }), name: nameOf(n) },
  ];
}

/** `list` with the place at `index` named `name`. */
export function renamed(list: readonly Place[], index: number, name: string): Place[] {
  return list.map((one, i) => (i === index ? { ...one, name } : one));
}

/** `list` with the place at `from` moved to `to`, within the list. */
export function moved(list: readonly Place[], from: number, to: number): Place[] {
  const at = Math.max(0, Math.min(list.length - 1, to));
  if (from === at || from < 0 || from >= list.length) return [...list];
  const rest = list.filter((_, i) => i !== from);
  return [...rest.slice(0, at), list[from]!, ...rest.slice(at)];
}

/** `list` without the place at `index`. */
export function removed(list: readonly Place[], index: number): Place[] {
  return list.filter((_, i) => i !== index);
}

/** The places as the server's positions: a blank name left out. */
export function positionsOf(list: readonly Place[]): ScanRequest["positions"] {
  return list.map((one) => ({
    x_um: one.x,
    y_um: one.y,
    ...(one.z === undefined ? {} : { z_um: one.z }),
    ...(one.name.trim() ? { name: one.name.trim() } : {}),
  }));
}

/** The planes a stack takes at each tile, as the server counts them: from its lowest, a step apart. */
export function planesOf(stack: Stack): number {
  if (!(stack.step_um > 0) || stack.below_um < 0 || stack.above_um < 0) return 0;
  return Math.floor((stack.below_um + stack.above_um) / stack.step_um + 1e-9) + 1;
}
