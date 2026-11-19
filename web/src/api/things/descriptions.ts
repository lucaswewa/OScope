// The Thing Descriptions (W1.2, ADR-0044): read once from
// `thing_descriptions/`, each Thing's properties, actions and events, and
// which properties may be written or observed.
import { call, server, type Server } from "../client.ts";

/** How an affordance is reached. */
export interface Form {
  href: string;
  op?: string | string[];
}

/** A property, as its Thing describes it, with its value's schema. */
export interface PropertyDescription {
  title?: string;
  description?: string;
  readOnly?: boolean;
  forms: Form[];
  [schema: string]: unknown;
}

/** A Thing, as it describes itself. */
export interface ThingDescription {
  id: string;
  title: string;
  description?: string;
  properties: Record<string, PropertyDescription>;
  actions: Record<string, { forms: Form[] }>;
  events: Record<string, { forms: Form[] }>;
}

const operations = (property: PropertyDescription) =>
  property.forms.flatMap((form) => (Array.isArray(form.op) ? form.op : form.op ? [form.op] : []));

/** Whether its Thing's WebSocket tells each change of it: data properties are, functional ones aren't. */
export const observable = (property: PropertyDescription): boolean =>
  operations(property).includes("observeproperty");

/** Whether a client may write it. */
export const writable = (property: PropertyDescription): boolean =>
  !property.readOnly && operations(property).includes("writeproperty");

const read = new WeakMap<Server, Promise<Record<string, ThingDescription>>>();

/** Every Thing's description, by its name: read once for each server, again only after a failure. */
export function descriptions(on: Server = server): Promise<Record<string, ThingDescription>> {
  let once = read.get(on);
  if (!once) {
    const answer: Promise<unknown> = call((signal) =>
      on.api.GET("/api/v1/thing_descriptions/", { signal }),
    );
    once = answer as Promise<Record<string, ThingDescription>>;
    once.catch(() => read.delete(on));
    read.set(on, once);
  }
  return once;
}

/** One Thing's description. */
export async function description(thing: string, on: Server = server): Promise<ThingDescription> {
  const found = (await descriptions(on))[thing];
  if (!found) throw new Error(`the server has no Thing called ${thing}`);
  return found;
}
