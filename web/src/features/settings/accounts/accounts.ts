// The accounts' view's arithmetic (W8.6, ADR-0055): what is wrong with a
// new account or a new password, as the server's rules say it, before it is
// sent; and how long something has been idle, in words' parts.

/** A password's least length (P49). */
export const LEAST = 8;

/** Why a new account's name can't be one: none if it can. */
export function nameWrong(name: string): "empty" | "form" | "local" | undefined {
  if (name === "") return "empty";
  if (name === "local") return "local";
  return /^[a-z][a-z0-9._-]{0,63}$/.test(name) ? undefined : "form";
}

/** Why a new password, typed twice, can't be one: none if it can. */
export function passwordWrong(password: string, again: string): "short" | "differ" | undefined {
  if ([...password].length < LEAST) return "short";
  return password === again ? undefined : "differ";
}

/** A time, its largest parts: seconds alone under a minute, else hours and minutes. */
export function spanOf(seconds: number): { h: number; min: number; s: number } {
  const whole = Math.max(0, Math.floor(seconds));
  if (whole < 60) return { h: 0, min: 0, s: whole };
  return { h: Math.floor(whole / 3600), min: Math.floor((whole % 3600) / 60), s: 0 };
}
