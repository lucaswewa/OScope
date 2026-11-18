// The pairs the tokens allow (ADR-0041): which token may be written or drawn
// on which. A test computes each pair's contrast, in both schemes and with
// every accent; the gallery shows each as a sample. A pair that isn't here
// isn't promised to read.

/** A token used as ink on another, and the least contrast ratio it must have. */
export interface Pair {
  /** The token written or drawn, without its `--`. */
  ink: string;
  /** The token behind it. */
  on: string;
  /** 4.5 for text, 3 for a line that marks a control (WCAG 2.2, 1.4.3 and 1.4.11). */
  least: 4.5 | 3;
}

const text = (ink: string, ...on: string[]): Pair[] =>
  on.map((surface) => ({ ink, on: surface, least: 4.5 }));
const line = (ink: string, ...on: string[]): Pair[] =>
  on.map((surface) => ({ ink, on: surface, least: 3 }));

/** Where a page's content may sit. */
const grounds = ["surface-page", "surface-pane", "surface-raised"];
/** Every surface text may be on. */
const surfaces = [...grounds, "surface-rail", "surface-field", "surface-open", "surface-hover"];

export const pairs: readonly Pair[] = [
  ...text("text", ...surfaces, "accent-tint"),
  ...text("text-muted", ...surfaces),
  ...text("text-title", ...grounds),
  ...text("text-inverse", "surface-inverse"),
  ...text("accent-on", "accent", "accent-hover", "accent-pressed"),
  ...text("accent-text", ...grounds, "surface-rail", "accent-tint"),
  ...text("info", ...grounds, "info-tint"),
  ...text("success", ...grounds, "success-tint"),
  ...text("warning", ...grounds, "warning-tint"),
  ...text("danger", ...grounds, "danger-tint"),
  ...text("danger-on", "danger-fill", "danger-fill-hover", "danger-fill-pressed"),
  ...line("border-control", ...grounds, "surface-field"),
  ...line("focus-color", ...surfaces),
];
