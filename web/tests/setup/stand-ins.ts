// Stand-ins for the server in unit tests (W1.2): a WebSocket whose server's
// side the test plays, and a network that answers by method and path.
import { vi } from "vitest";

/** A WebSocket the test drives: it opens, says and drops when told. */
export class FakeSocket extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  /** Every socket made, in order. */
  static made: FakeSocket[] = [];
  readyState = FakeSocket.CONNECTING;
  /** What the client sent, each parsed. */
  readonly sent: unknown[] = [];

  constructor(readonly url: string) {
    super();
    FakeSocket.made.push(this);
  }

  send(text: string): void {
    this.sent.push(JSON.parse(text));
  }

  close(): void {
    if (this.readyState === FakeSocket.CLOSED) return;
    this.readyState = FakeSocket.CLOSED;
    this.dispatchEvent(new Event("close"));
  }

  /** The server accepts it. */
  opens(): void {
    this.readyState = FakeSocket.OPEN;
    this.dispatchEvent(new Event("open"));
  }

  /** The server sends a message. */
  says(message: unknown): void {
    this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(message) }));
  }

  /** The server goes away. */
  drops(): void {
    this.close();
  }

  /** The newest socket made. */
  static get last(): FakeSocket {
    return FakeSocket.made.at(-1)!;
  }
}

/** Puts FakeSocket in WebSocket's place, with no socket made yet. */
export function standInSockets(): void {
  FakeSocket.made = [];
  vi.stubGlobal("WebSocket", FakeSocket);
}

const json = (value: unknown, status = 200) =>
  new Response(value === undefined ? null : JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });

/** An answer, from a route: a Response as it is, anything else as JSON. */
type Route = (request: Request) => unknown;

/**
 * Puts a network in fetch's place that answers `METHOD /path` from `routes`,
 * and 404 otherwise: the requests made are kept.
 */
export function standInNetwork(routes: Record<string, Route>): Request[] {
  const made: Request[] = [];
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    made.push(request);
    const route = routes[`${request.method} ${new URL(request.url).pathname}`];
    if (!route) return json({ detail: "Not Found" }, 404);
    const answer = await route(request);
    return answer instanceof Response ? answer : json(answer);
  });
  return made;
}

/** A problem, as the server answers a refusal. */
export const problem = (status: number, title: string, detail: string) =>
  json({ status, title, detail, type: null, instance: null }, status);

/** A Thing's description, with what the tests need of it. */
export function described(
  properties: Record<string, { observable?: boolean; writable?: boolean }>,
  events: string[],
) {
  return {
    id: "urn:a-thing",
    title: "A Thing",
    properties: Object.fromEntries(
      Object.entries(properties).map(([name, { observable, writable }]) => [
        name,
        {
          readOnly: !writable,
          forms: [
            { href: name, op: writable ? ["readproperty", "writeproperty"] : ["readproperty"] },
            ...(observable ? [{ href: name, op: ["observeproperty", "unobserveproperty"] }] : []),
          ],
        },
      ]),
    ),
    actions: {},
    events: Object.fromEntries(events.map((name) => [name, { forms: [] }])),
  };
}
