// The themes and accents the tokens offer (ADR-0041), and how a choice
// reaches them: as attributes on the root, which tokens.css reads.

/** The theme: the computer's own, or one of the two. */
export const themes = ["system", "light", "dark"] as const;
export type Theme = (typeof themes)[number];

/** The accents. The first is the one a root without a choice has. */
export const accents = [
  "magenta",
  "amaranth",
  "violet",
  "fern",
  "mantis",
  "hunter",
  "midnight",
  "dusk",
  "harbor",
] as const;
export type Accent = (typeof accents)[number];

/** A colour scheme: what a theme comes to. */
export type Scheme = "light" | "dark";

/** Whether `value` is one of `known`: for what storage, or a server, hands back. */
export function isOneOf<T extends string>(known: readonly T[], value: unknown): value is T {
  return typeof value === "string" && (known as readonly string[]).includes(value);
}

/**
 * Puts a theme and an accent on the root. System is no attribute at all:
 * the page then follows the computer's scheme by itself, as it changes.
 */
export function apply(root: HTMLElement, theme: Theme, accent: Accent): void {
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
  root.setAttribute("data-accent", accent);
}
