// What the browser keeps for its session (W1.3): the login's token, kept
// until the tab is closed, so that a reload stays logged in. Through here
// only: the browser's storage is the host's (ADR-0039).
const KEY = "oscope.session";

/** The token kept, if any: none where the browser keeps nothing. */
export function keptToken(): string | undefined {
  try {
    return sessionStorage.getItem(KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

/** Keeps a token for the session, or forgets it. */
export function keepToken(token: string | undefined): void {
  try {
    if (token) sessionStorage.setItem(KEY, token);
    else sessionStorage.removeItem(KEY);
  } catch {
    // Not kept: the next visit logs in again.
  }
}
