// The app's own third-party notices (W4.3): `notices.json`, which the build
// puts beside the app (vite.config.ts, scripts/notices.mjs), read from the
// app's own origin, not the server's.

/** A package the app's bundle holds, and its licence. */
export interface AppNotice {
  name: string;
  version: string;
  /** Its licence, as an SPDX expression. */
  licence: string;
  /** Its licence files' text; empty when it has none. */
  text: string;
}

/** The app's notices, in order of the packages' names. */
export async function appNotices(): Promise<AppNotice[]> {
  const answer = await fetch(new URL("notices.json", document.baseURI));
  if (!answer.ok) throw new Error(`notices.json: ${answer.status}`);
  return (await answer.json()) as AppNotice[];
}
