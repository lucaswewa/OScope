// What a recipe costs live (W7.3): each step's time, and the whole's, as
// the server measured them when it was set, each against the live budget.

export interface Cost {
  /** Its time in ms, as it is shown: tenths below 10 ms, whole ms from there. */
  ms: string;
  /** Its share of the budget, in whole %. */
  percent: number;
  /** Whether it alone is over the budget. */
  over: boolean;
}

/** `ms` against `budget`, both in ms. */
export function costOf(ms: number, budget: number): Cost {
  const shown = ms < 10 ? ms.toFixed(1) : Math.round(ms).toString();
  const percent = budget > 0 ? Math.round((100 * ms) / budget) : Infinity;
  return { ms: shown, percent, over: ms > budget };
}

/** Each step's cost, by its ID. */
export function costsOf(
  steps: Readonly<Record<string, number>>,
  budget: number,
): ReadonlyMap<string, Cost> {
  return new Map(Object.entries(steps).map(([id, ms]) => [id, costOf(ms, budget)]));
}
