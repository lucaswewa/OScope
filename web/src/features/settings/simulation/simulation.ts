// The simulator's view's arithmetic (W8.4): each built-in sample's picture
// and the key of what it is for; and the simulator's settings for the next
// start as a form, its fields flat, checked, and written back over the
// document it came from, which keeps what the form doesn't show.

/** The simulator's settings as the configuration file holds them (`[simulator]`). */
export interface SimulatorSettings {
  rate: number;
  sensor: string;
  objective: string;
  sample: string;
  focus: number;
  tilt: [number, number];
  rotation: number;
  mirrored: boolean;
  scale: number;
  backlash: number;
  latency: number;
  encoders: boolean;
  lost_steps: [number, number];
  vignetting: number;
  gpu: boolean;
  /** What the form doesn't show, the colour response and one's own samples among it, kept. */
  [other: string]: unknown;
}

/** The form: the settings it shows, a pair's halves apart. */
export interface Form {
  sensor: string;
  objective: string;
  sample: string;
  focus: number;
  tiltX: number;
  tiltY: number;
  rotation: number;
  mirrored: boolean;
  scale: number;
  backlash: number;
  latency: number;
  encoders: boolean;
  lostChance: number;
  lostUm: number;
  vignetting: number;
  gpu: boolean;
}

/** `settings` as the form shows them. */
export function formOf(settings: SimulatorSettings): Form {
  const { sensor, objective, sample, focus, rotation, mirrored, scale } = settings;
  const { backlash, latency, encoders, vignetting, gpu } = settings;
  return {
    ...{ sensor, objective, sample, focus, rotation, mirrored, scale },
    ...{ backlash, latency, encoders, vignetting, gpu },
    tiltX: settings.tilt[0],
    tiltY: settings.tilt[1],
    lostChance: settings.lost_steps[0],
    lostUm: settings.lost_steps[1],
  };
}

/** `form` written over `settings`, the document it came from: what it doesn't show, kept. */
export function settingsOf(form: Form, settings: SimulatorSettings): SimulatorSettings {
  const { tiltX, tiltY, lostChance, lostUm, ...plain } = form;
  return { ...settings, ...plain, tilt: [tiltX, tiltY], lost_steps: [lostChance, lostUm] };
}

/**
 * What in `form` can't be, by its first wrong field; none if all can. The
 * server refuses a scale of 0 or less, and a negative backlash or
 * vignetting, too.
 */
export function wrongIn(form: Form): { field: keyof Form; why: string } | undefined {
  const numbers = Object.entries(form).filter(([, value]) => typeof value === "number");
  const unset = numbers.find(([, value]) => !Number.isFinite(value));
  if (unset) return { field: unset[0] as keyof Form, why: "number" };
  if (form.scale <= 0) return { field: "scale", why: "positive" };
  for (const field of ["backlash", "latency", "vignetting", "lostUm"] as const) {
    if (form[field] < 0) return { field, why: "notNegative" };
  }
  if (form.lostChance < 0 || form.lostChance > 1) return { field: "lostChance", why: "chance" };
  return undefined;
}

/** A sample's key: its name in lower case, its words joined by `-`, as its picture is named. */
export const slugOf = (name: string) => name.toLowerCase().replace(/[^a-z0-9]+/g, "-");

// The built-in samples' pictures, recorded by `scripts/record-samples.mjs`.
const pictures = import.meta.glob<string>("./pictures/*.jpg", {
  eager: true,
  query: "?url",
  import: "default",
});

/** A built-in sample's picture's address; none for one's own. */
export function pictureOf(name: string): string | undefined {
  return pictures[`./pictures/${slugOf(name)}.jpg`];
}
