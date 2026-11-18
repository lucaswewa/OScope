/**
 * The server's version, from the `system` Thing. The address is relative:
 * the app is served by its server, or by a dev server that passes `/api` on.
 */
export async function serverVersion(signal: AbortSignal): Promise<string> {
  const answer = await fetch("api/v1/system/version", { signal });
  if (!answer.ok) {
    throw new Error(`system/version: ${answer.status}`);
  }
  return (await answer.json()) as string;
}
