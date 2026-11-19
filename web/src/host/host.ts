// The host the app has (ADR-0046): the browser's, unless another was
// installed in its place: the desktop app's, at its start (W9.1), or a
// test's.
import type { HostAdapter } from "./adapter";
import { browserHost } from "./browser";

let installed: HostAdapter | undefined;

/** The platform's services. */
export function host(): HostAdapter {
  installed ??= browserHost(window);
  return installed;
}

/** Puts `adapter` in the browser's place; given nothing, the browser's comes back. */
export function installHost(adapter: HostAdapter | undefined): void {
  installed = adapter;
}
