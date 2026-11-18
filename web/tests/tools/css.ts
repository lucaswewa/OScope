// Reads the tokens' stylesheet as a browser would, far enough to measure it
// (ADR-0041): the custom properties a root has under a theme and an accent,
// each colour worked out for a scheme, and WCAG 2.2's contrast between two.

export type Scheme = "light" | "dark";

/** A colour's red, green and blue, each from 0 to 1. */
export type Rgb = readonly [number, number, number];

export interface Rule {
  /** The media query the rule is under, or none. */
  media: string | null;
  selectors: string[];
  properties: Map<string, string>;
}

/** A stylesheet's rules, in order. Only `@media` is looked into. */
export function rules(css: string): Rule[] {
  const out: Rule[] = [];
  const walk = (block: string, media: string | null): void => {
    let at = 0;
    for (;;) {
      const open = block.indexOf("{", at);
      if (open < 0) return;
      let end = open + 1;
      for (let depth = 1; depth > 0 && end < block.length; end++) {
        if (block[end] === "{") depth++;
        else if (block[end] === "}") depth--;
      }
      const head = block.slice(at, open).trim();
      const body = block.slice(open + 1, end - 1);
      if (head.startsWith("@media")) walk(body, head.slice("@media".length).trim());
      else if (!head.startsWith("@")) {
        const selectors = head.split(",").map((selector) => selector.trim());
        out.push({ media, selectors, properties: declarations(body) });
      }
      at = end;
    }
  };
  walk(css.replace(/\/\*[\s\S]*?\*\//g, ""), null);
  return out;
}

function declarations(body: string): Map<string, string> {
  const properties = new Map<string, string>();
  for (const declaration of body.split(";")) {
    const colon = declaration.indexOf(":");
    if (colon < 0) continue;
    const value = declaration
      .slice(colon + 1)
      .replace(/\s+/g, " ")
      .trim();
    properties.set(declaration.slice(0, colon).trim(), value);
  }
  return properties;
}

/** How many of a root's attributes a selector names, if it is the root's: `:root[a="b"]`. */
function matches(selector: string, attributes: Record<string, string>): number | null {
  if (!selector.startsWith(":root")) return null;
  const named = [...selector.slice(":root".length).matchAll(/\[([\w-]+)="([^"]*)"\]/g)];
  if (named.map((part) => part[0]).join("") !== selector.slice(":root".length)) return null;
  return named.every((part) => attributes[part[1]!] === part[2]) ? named.length : null;
}

/**
 * What a root with these attributes has, under these media queries: of the
 * rules that match it, the more specific wins, and then the later.
 */
export function root(
  sheet: Rule[],
  attributes: Record<string, string>,
  media: string[] = [],
): Map<string, string> {
  const matching: { weight: number; properties: Map<string, string> }[] = [];
  for (const rule of sheet) {
    if (rule.media !== null && !media.includes(rule.media)) continue;
    const weights = rule.selectors.map((selector) => matches(selector, attributes));
    const weight = Math.max(...weights.map((one) => one ?? -1));
    if (weight >= 0) matching.push({ weight, properties: rule.properties });
  }
  // A stable sort: rules of one weight keep the stylesheet's order.
  matching.sort((a, b) => a.weight - b.weight);
  return new Map(matching.flatMap((rule) => [...rule.properties]));
}

/** The scheme a root's `color-scheme` comes to, on a computer of this scheme. */
export function schemeOf(properties: Map<string, string>, computer: Scheme): Scheme {
  const offered = (properties.get("color-scheme") ?? "light").split(" ");
  if (offered.includes(computer)) return computer;
  return offered.includes("dark") ? "dark" : "light";
}

/** A function's arguments: split at the commas that aren't inside another's. */
function split(inside: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let from = 0;
  for (let at = 0; at < inside.length; at++) {
    if (inside[at] === "(") depth++;
    else if (inside[at] === ")") depth--;
    else if (inside[at] === "," && depth === 0) {
      out.push(inside.slice(from, at).trim());
      from = at + 1;
    }
  }
  return [...out, inside.slice(from).trim()];
}

const hex = (digits: string): Rgb => {
  const full = digits.length === 3 ? [...digits].map((digit) => digit + digit).join("") : digits;
  const channel = (at: number): number => parseInt(full.slice(at, at + 2), 16) / 255;
  return [channel(0), channel(2), channel(4)];
};

/**
 * A value's colour in a scheme, worked out as a browser does: `var()` by the
 * token it names, `light-dark()` by the scheme, `color-mix(in srgb, …)`
 * channel by channel. Only opaque colours: one with alpha can't be measured.
 */
export function colour(value: string, properties: Map<string, string>, scheme: Scheme): Rgb {
  const text = value.replace(/\s+/g, " ").trim();
  const literal = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text);
  if (literal) return hex(literal[1]!);
  if (text === "white") return [1, 1, 1];
  if (text === "black") return [0, 0, 0];
  const called = /^([a-z-]+)\((.*)\)$/.exec(text);
  if (!called) throw new Error(`not an opaque colour: ${text}`);
  const args = split(called[2]!);
  switch (called[1]) {
    case "var": {
      const named = properties.get(args[0]!);
      if (named === undefined) throw new Error(`no such token: ${args[0]}`);
      return colour(named, properties, scheme);
    }
    case "light-dark":
      if (args.length !== 2) throw new Error(`light-dark() takes two colours: ${text}`);
      return colour(args[scheme === "light" ? 0 : 1]!, properties, scheme);
    case "color-mix": {
      if (args.length !== 3 || args[0] !== "in srgb")
        throw new Error(`only color-mix(in srgb, a, b): ${text}`);
      const parts = args.slice(1).map((part) => {
        const share = /(?:^| )(\d+(?:\.\d+)?)%(?: |$)/.exec(part);
        const of = share ? part.replace(share[0], " ").trim() : part;
        return { share: share ? Number(share[1]) : null, rgb: colour(of, properties, scheme) };
      });
      const [a, b] = [parts[0]!, parts[1]!];
      // CSS Color 5: a share left out is the rest of the other's, or a half each.
      const first = a.share ?? (b.share === null ? 50 : 100 - b.share);
      const second = b.share ?? 100 - first;
      if (first + second < 100) throw new Error(`shares under 100 % leave alpha: ${text}`);
      const share = first / (first + second);
      return [0, 1, 2].map((at) => a.rgb[at]! * share + b.rgb[at]! * (1 - share)) as unknown as Rgb;
    }
    default:
      throw new Error(`not an opaque colour: ${text}`);
  }
}

/** A colour's relative luminance (WCAG 2.2), from sRGB's channels made linear. */
export function luminance(rgb: Rgb): number {
  const [r, g, b] = rgb.map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

/** The contrast ratio between two colours (WCAG 2.2): from 1 to 21. */
export function contrast(a: Rgb, b: Rgb): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter! + 0.05) / (darker! + 0.05);
}
