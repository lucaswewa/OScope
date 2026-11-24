// The Settings tab's arithmetic (W8.1): what changed in a draft of the
// system's settings; the settings document's file, and what a file read
// back holds; and a camera's saved settings, said in words.
import type { Schema } from "@/api/client";

/** What the server says a settings document is. */
export const FORMAT = "oscope.settings";

/** The names of the parts of `draft` that differ from `saved`, in `saved`'s order. */
export function changedParts<T extends object>(saved: T, draft: T): string[] {
  return (Object.keys(saved) as (keyof T & string)[]).filter((key) => saved[key] !== draft[key]);
}

/** The file a settings document is exported to, by the day it was. */
export function documentName(now: Date): string {
  const day = [now.getFullYear(), now.getMonth() + 1, now.getDate()]
    .map((part) => String(part).padStart(2, "0"))
    .join("-");
  return `oscope-settings-${day}.json`;
}

/** What a file read back holds: a settings document, or why it isn't one. */
export function documentOf(
  text: string,
): { document: Record<string, unknown> } | { why: "notJson" | "notSettings" } {
  let read: unknown;
  try {
    read = JSON.parse(text);
  } catch {
    return { why: "notJson" };
  }
  const object = typeof read === "object" && read !== null && !Array.isArray(read);
  if (!object || (read as Record<string, unknown>).format !== FORMAT) return { why: "notSettings" };
  return { document: read as Record<string, unknown> };
}

type CameraSaved = NonNullable<Schema<"camera_saved_value">>;

/** A camera's saved settings, each a word and its value, the units given by `words`. */
export function cameraSaid(
  saved: CameraSaved,
  words: { all: string; trigger: (kind: string) => string },
): { term: "mode" | "format" | "exposure" | "gain" | "roi" | "trigger"; value: string }[] {
  const roi = saved.roi
    ? `${saved.roi.x}, ${saved.roi.y}: ${saved.roi.width} × ${saved.roi.height}`
    : words.all;
  const edge = saved.trigger.kind === "external" ? `, ${saved.trigger.edge}` : "";
  return [
    { term: "mode", value: saved.mode },
    { term: "format", value: saved.format.kind },
    { term: "exposure", value: `${Number(saved.exposure.toPrecision(4))} ms` },
    { term: "gain", value: String(saved.gain) },
    { term: "roi", value: roi },
    { term: "trigger", value: words.trigger(saved.trigger.kind) + edge },
  ];
}
