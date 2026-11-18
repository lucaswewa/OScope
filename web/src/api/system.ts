import { call, server } from "./client.ts";

/**
 * The server's version, from the `system` Thing, with the typed client. The
 * address is relative: the app is served by its server, or by a dev server
 * that passes `/api` on.
 */
export function serverVersion(signal: AbortSignal): Promise<string> {
  return call((stop) => server.api.GET("/api/v1/system/version", { signal: stop }), { signal });
}
