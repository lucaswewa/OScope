// Stains separated (W7.6, P46): what each of a deconvolution's channels
// is, the vectors used, and DAB's score. The preset vectors are Ruifrok and
// Johnston's (2001), as the server's are; a recipe may give its own.

type Vector = [number, number, number];

/** Optical densities in red, green and blue, for a unit of each stain (Ruifrok and Johnston, 2001). */
export const VECTORS: Record<string, Vector> = {
  Hematoxylin: [0.65, 0.704, 0.286],
  Eosin: [0.072, 0.99, 0.105],
  DAB: [0.268, 0.57, 0.776],
};

/** A deconvolution's stains: their names, in its channels' order, and their vectors. */
export interface Stains {
  names: string[];
  vectors: Vector[];
}

/** The stains `stain.deconvolve` was given: `he`, `hdab`, or its own vectors. */
export function stainsOf(choice: unknown): Stains {
  if (choice === "hdab")
    return {
      names: ["Hematoxylin", "DAB", "Residual"],
      vectors: [VECTORS.Hematoxylin!, VECTORS.DAB!],
    };
  const own = (choice as { vectors?: unknown } | null)?.vectors;
  if (Array.isArray(own))
    return {
      names: own.map((_, i) => `Stain ${i + 1}`).concat(own.length < 3 ? ["Residual"] : []),
      vectors: own as Vector[],
    };
  return {
    names: ["Hematoxylin", "Eosin", "Residual"],
    vectors: [VECTORS.Hematoxylin!, VECTORS.Eosin!],
  };
}

/** A recipe's deconvolution's stains, if it has one. */
export function deconvolved(recipe: unknown): Stains | undefined {
  const nodes = (recipe as { nodes?: { node?: string; params?: { stains?: unknown } }[] } | null)
    ?.nodes;
  const node = nodes?.find((each) => each.node === "stain.deconvolve");
  return node ? stainsOf(node.params?.stains) : undefined;
}

/** DAB's score, `stain.dab_score`'s one row: its pixels by class, the positive %, and the H-score. */
export interface Score {
  glass: number;
  negative: number;
  weak: number;
  moderate: number;
  strong: number;
  tissue: number;
  positive: number;
  hScore: number;
}

/** The score from its row. */
export function scoreOf(row: Record<string, string | number> | undefined): Score | undefined {
  if (!row) return undefined;
  const n = (name: string) => Number(row[name] ?? 0);
  return {
    glass: n("glass"),
    negative: n("negative"),
    weak: n("weak"),
    moderate: n("moderate"),
    strong: n("strong"),
    tissue: n("tissue"),
    positive: n("positive %"),
    hScore: n("H-score"),
  };
}
