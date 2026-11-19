// The session (W1.3, ADR-0031): who the app is, when the server has
// accounts. Logging in gives a token, which goes with every call; logging
// out takes it away, and so does a session the server ended, noticed at the
// next call's 401. Without accounts there is no session to have: everyone
// is let in.
import { defineStore } from "pinia";
import { shallowRef } from "vue";

import { call, fetched, onUnauthorised, server, session, type Server } from "../client.ts";
import { ApiError } from "../errors.ts";

/** What logging in answers. */
export interface LoggedIn {
  /** The session's token. */
  token: string;
  name: string;
}

/**
 * Logs in at a server: its session's token. The login and the logout are
 * the server's own endpoints, described without their bodies, so they are
 * typed here. A wrong name or password is thrown, as the server's 401.
 */
export function logIn(name: string, password: string, on: Server = server): Promise<LoggedIn> {
  return fetched(
    on.address("/api/v1/auth/login"),
    (answer) => answer.json() as Promise<LoggedIn>,
    {},
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name, password }),
    },
  );
}

/** Whether the app is let in. */
export type SessionState =
  /** Not asked yet. */
  | "unknown"
  /** The server has no accounts: everyone is let in. */
  | "open"
  /** It has, and the app isn't logged in. */
  | "out"
  /** Logged in. */
  | "in";

export const useSession = defineStore("session", () => {
  const state = shallowRef<SessionState>("unknown");
  const name = shallowRef<string>();
  /** Whether the last session ended on the server, rather than by logging out. */
  const ended = shallowRef(false);
  const keepers = new Set<(token: string | undefined) => void>();

  function hold(token: string | undefined): void {
    session.token = token;
    for (const keeper of keepers) keeper(token);
  }

  onUnauthorised(() => {
    if (state.value === "open") return;
    if (state.value === "in") ended.value = true;
    hold(undefined);
    name.value = undefined;
    state.value = "out";
  });

  /** Asks the server who the app is: logged in, out, or let in without accounts. */
  async function probe(): Promise<SessionState> {
    try {
      const me: unknown = await call((signal) => server.api.GET("/api/v1/auth/me", { signal }));
      name.value = (me as { name: string }).name;
      state.value = "in";
    } catch (thrown) {
      // No `auth` Thing: the server has no accounts.
      if (thrown instanceof ApiError && thrown.status === 404) state.value = "open";
      else if (thrown instanceof ApiError && thrown.status === 401) state.value = "out";
      else throw thrown;
    }
    return state.value;
  }

  /** Logs in: a wrong name or password is thrown, as the server's 401. */
  async function login(user: string, password: string): Promise<void> {
    const answer = await logIn(user, password);
    hold(answer.token);
    name.value = answer.name;
    ended.value = false;
    state.value = "in";
  }

  /** Logs out: the session ends on the server, and here. */
  async function logout(): Promise<void> {
    try {
      await fetched(
        server.address("/api/v1/auth/logout"),
        async () => undefined,
        {},
        {
          method: "POST",
        },
      );
    } finally {
      hold(undefined);
      name.value = undefined;
      ended.value = false;
      state.value = "out";
    }
  }

  /** Takes up a token kept from before, if the server still knows it. */
  async function resume(token: string): Promise<SessionState> {
    session.token = token;
    const now = await probe().catch(() => state.value);
    if (now !== "in") session.token = undefined;
    return now;
  }

  /** Told each token given or taken away, for the host to keep. */
  function keepWith(keeper: (token: string | undefined) => void): () => void {
    keepers.add(keeper);
    return () => keepers.delete(keeper);
  }

  return { state, name, ended, probe, login, logout, resume, keepWith };
});
