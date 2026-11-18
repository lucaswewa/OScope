import { onScopeDispose, readonly, ref, type Ref } from "vue";

/** What the page knows of the server. */
export type ServerState =
  { kind: "connecting" } | { kind: "connected"; version: string } | { kind: "unreachable" };

/**
 * Asks for the server's version now, and again every `every` milliseconds,
 * so the state follows a server that stops and one that comes back. An
 * answer that takes longer than `every` counts as none.
 */
export function useServerVersion(
  ask: (signal: AbortSignal) => Promise<string>,
  every = 2000,
): Readonly<Ref<ServerState>> {
  const state = ref<ServerState>({ kind: "connecting" });
  let stopped = false;
  let request: AbortController | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function look(): Promise<void> {
    request = new AbortController();
    const mine = request;
    const limit = setTimeout(() => mine.abort(), every);
    try {
      const version = await ask(mine.signal);
      if (!stopped) state.value = { kind: "connected", version };
    } catch {
      if (!stopped) state.value = { kind: "unreachable" };
    } finally {
      clearTimeout(limit);
    }
    if (!stopped) timer = setTimeout(() => void look(), every);
  }

  void look();
  onScopeDispose(() => {
    stopped = true;
    request?.abort();
    clearTimeout(timer);
  });
  return readonly(state);
}
