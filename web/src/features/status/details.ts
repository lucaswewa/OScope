// The status in full (W4.3): what the server measured of each subsystem, as
// terms and values. The known measures are named, and given in their units,
// which the end of a measure's name says: the GPU's memory and temperature,
// the scratch disk's room and how fast it fills, the jobs, the extensions'
// problems. Any other is shown as it comes.

/** A measure's unit, by the end of its name, and the digits it is given with. */
const UNITS: readonly [ending: string, unit: string, digits: number][] = [
  ["_mib", "MiB", 0],
  ["_gb", "GB", 1],
  ["_mb_s", "MB/s", 0],
  ["_c", "°C", 0],
];

/**
 * A subsystem's details as terms and values: each measure's word, if `word`
 * has one, or its name in words; its value in its unit. None for details
 * that aren't an object of measures.
 */
export function detailsOf(
  details: unknown,
  word: (key: string) => string | undefined,
): { term: string; value: string }[] {
  if (details === null || typeof details !== "object" || Array.isArray(details)) return [];
  return Object.entries(details).map(([key, value]) => ({
    term: word(key) ?? key.replaceAll("_", " "),
    value: valueOf(key, value),
  }));
}

function valueOf(key: string, value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (Array.isArray(value)) return value.map(String).join("; ");
  if (typeof value === "number") {
    const unit = UNITS.find(([ending]) => key.endsWith(ending));
    return unit ? `${value.toFixed(unit[2])} ${unit[1]}` : String(value);
  }
  return String(value);
}
