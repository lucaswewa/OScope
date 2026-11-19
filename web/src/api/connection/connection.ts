// The connection (W1.3, ADR-0045): whether the app has its server. The
// server's `/health`, open to anyone, is asked every 2 s; its version is
// checked against the app's once the server lets the app read it, at once
// without accounts, after the login with them.
import { defineStore } from "pinia";
import { computed, shallowRef } from "vue";

import { version } from "../../../package.json";
import { call, fetched, server, type Server } from "../client.ts";
import { ApiError } from "../errors.ts";

/** Where the connection is. */
export type ConnectionState =
  /** No answer yet. */
  | "connecting"
  /** The server answers, in a version the app knows. */
  | "connected"
  /** It answered before, and doesn't now: the app keeps asking. */
  | "reconnecting"
  /** It answers, and refuses the app: `/health` isn't well, or the app's origin isn't let in. */
  | "refused"
  /** It answers, in a version the app doesn't know. */
  | "unknown version";

/** How often `/health` is asked. */
export const WATCH_EVERY = 2_000;

/** The app's version. */
export const APP_VERSION = version;

/** Whether the app knows a server's version: the same major and minor numbers as its own. */
export function knows(serverVersion: string, appVersion = APP_VERSION): boolean {
  const [ours, theirs] = [appVersion, serverVersion].map((v) => v.split(".").slice(0, 2).join("."));
  return ours === theirs;
}

export const useConnection = defineStore("connection", () => {
  /** The last look's answer: none yet, well, refused, or silent. */
  const last = shallowRef<"none" | "well" | "refused" | "silent">("none");
  const ever = shallowRef(false);
  const serverVersion = shallowRef<string>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let on: Server = server;

  const state = computed<ConnectionState>(() => {
    switch (last.value) {
      case "well":
        return serverVersion.value === undefined || knows(serverVersion.value)
          ? "connected"
          : "unknown version";
      case "refused":
        return "refused";
      case "silent":
        return ever.value ? "reconnecting" : "connecting";
      default:
        return "connecting";
    }
  });

  /** Reads the server's version, if the server lets the app: under security, once logged in. */
  async function readVersion(): Promise<void> {
    try {
      serverVersion.value = await call((signal) =>
        on.api.GET("/api/v1/system/version", { signal }),
      );
    } catch {
      // Not yet: after the login.
    }
  }

  /** Asks `/health` once. */
  async function look(): Promise<void> {
    try {
      await fetched(on.address("/health"), (answer) => answer.json(), { timeout: WATCH_EVERY });
      last.value = "well";
      ever.value = true;
      if (serverVersion.value === undefined) await readVersion();
    } catch (thrown) {
      // An answer that isn't well is a refusal; no answer is silence.
      last.value = thrown instanceof ApiError && thrown.status !== undefined ? "refused" : "silent";
    }
  }

  /** Watches the server at `at`: the function returned stops. */
  function watch(at: Server = server): () => void {
    on = at;
    clearInterval(timer);
    void look();
    timer = setInterval(() => void look(), WATCH_EVERY);
    return () => clearInterval(timer);
  }

  return { state, serverVersion, watch, look, readVersion };
});
