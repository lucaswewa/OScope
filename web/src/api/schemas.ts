// The schemas the server publishes (W0.6): each Thing's properties, its
// actions' inputs and outputs, and its events' data, from the Thing
// Descriptions in its directory; each pipeline node's parameters; and each
// scan pattern's that has its own. The addresses are relative, as in `system.ts`.

export interface Published {
  /** Unique: `camera/set_mode/input`. */
  id: string;
  /** For people: `camera · set_mode · input`. */
  label: string;
  /** Where it is from. */
  part: "property" | "input" | "output" | "event" | "node" | "pattern";
  schema: unknown;
  /** An action's input: where a value is sent, for the server to take or refuse. */
  href?: string;
}

/** What an affordance says of how it is reached, apart from the value it holds. */
const REACHED = new Set([
  "forms",
  "observable",
  "uriVariables",
  "op",
  "safe",
  "idempotent",
  "synchronous",
  "@type",
  "titles",
  "descriptions",
]);

type Record_ = Record<string, unknown>;
const isRecord = (value: unknown): value is Record_ =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const entries = (value: unknown): [string, Record_][] =>
  isRecord(value)
    ? Object.entries(value).filter((pair): pair is [string, Record_] => isRecord(pair[1]))
    : [];

/** A property's data schema: its affordance, without what says how it is reached. */
const valueOf = (affordance: Record_): Record_ =>
  Object.fromEntries(Object.entries(affordance).filter(([key]) => !REACHED.has(key)));

/** Every schema in some Thing Descriptions, by Thing and affordance. */
export function publishedIn(descriptions: unknown[]): Published[] {
  const found: Published[] = [];
  for (const td of descriptions.filter(isRecord)) {
    const thing = typeof td.title === "string" ? td.title : String(td.id ?? "");
    const add = (name: string, part: Published["part"], schema: unknown, href?: string): void => {
      const one: Published = {
        id: `${thing}/${name}/${part}`,
        label: `${thing} · ${name} · ${part}`,
        part,
        schema,
      };
      if (href) one.href = href;
      found.push(one);
    };
    for (const [name, property] of entries(td.properties)) add(name, "property", valueOf(property));
    for (const [name, action] of entries(td.actions)) {
      const forms = Array.isArray(action.forms) ? action.forms.filter(isRecord) : [];
      const invoke = forms.find(
        (form) => !Array.isArray(form.op) || form.op.includes("invokeaction"),
      );
      const href = typeof invoke?.href === "string" ? invoke.href.replace(/^\//, "") : undefined;
      if (isRecord(action.input)) add(name, "input", action.input, href);
      if (isRecord(action.output)) add(name, "output", action.output);
    }
    for (const [name, event] of entries(td.events)) {
      if (isRecord(event.data)) add(name, "event", event.data);
    }
  }
  return found;
}

async function read(path: string, signal: AbortSignal): Promise<unknown> {
  const answer = await fetch(path, { signal });
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

/** Every schema the server publishes, its Things' first. */
export async function published(signal: AbortSignal): Promise<Published[]> {
  const [things, nodes, patterns] = await Promise.all([
    read("api/v1/directory/things", signal),
    read("api/v1/preview/nodes", signal),
    read("api/v1/scan/patterns", signal),
  ]);
  const found = publishedIn(Array.isArray(things) ? things : []);
  for (const node of Array.isArray(nodes) ? nodes.filter(isRecord) : []) {
    const id = String(node.id);
    found.push({ id: `node/${id}`, label: `node · ${id}`, part: "node", schema: node.params });
  }
  for (const pattern of Array.isArray(patterns) ? patterns.filter(isRecord) : []) {
    if (!isRecord(pattern.schema)) continue;
    const name = String(pattern.name);
    found.push({
      id: `pattern/${name}`,
      label: `pattern · ${name}`,
      part: "pattern",
      schema: pattern.schema,
    });
  }
  return found;
}

/**
 * Sends a value to an action, for the server to take or refuse: whether it
 * took it, and its answer. A value the server takes is acted on.
 */
export async function offer(
  href: string,
  value: unknown,
): Promise<{ taken: boolean; answer: unknown }> {
  const response = await fetch(href, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(value),
  });
  const answer: unknown = await response.json().catch(() => null);
  return { taken: response.ok, answer };
}
