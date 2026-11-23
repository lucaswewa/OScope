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
  binaryType: BinaryType = "blob";
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

  /** The server sends a binary message. */
  sendsBytes(data: ArrayBuffer): void {
    this.dispatchEvent(new MessageEvent("message", { data }));
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

/** What a preview message made by hand holds (ADR-0014, ADR-0035). */
export interface MadeMessage {
  version?: number;
  kind?: number;
  /** The header's length: 76, 72 for ADR-0014's first, more for fields to come. */
  length?: number;
  sequence?: number;
  jpeg?: number[];
  /** Each block: its kind and its bytes. */
  blocks?: Array<[number, Uint8Array]>;
}

/**
 * A preview message, made by hand: frame 42 of a 640 × 480 region at
 * (10, 20), shown at 320 × 240, its exposure ending at 21 ms and its
 * message ready at 45 ms.
 */
export function previewMessage(made: MadeMessage = {}): ArrayBuffer {
  const { version = 1, kind = 1, length = 76, sequence = 0 } = made;
  const { jpeg = [0xff, 0xd8, 0xff, 0xd9], blocks = [] } = made;
  const size = blocks.reduce((sum, [, bytes]) => sum + 8 + bytes.length, length + jpeg.length);
  const buffer = new ArrayBuffer(size);
  const [view, all] = [new DataView(buffer), new Uint8Array(buffer)];
  all.set([..."OSPV"].map((letter) => letter.charCodeAt(0)));
  [version, kind, 1, 80].forEach((byte, at) => view.setUint8(4 + at, byte));
  view.setUint32(8, length, true);
  view.setUint32(12, sequence, true);
  [42n, 1_000_000n, 21_000_000n, 45_000_000n].forEach((n, at) =>
    view.setBigUint64(16 + 8 * at, n, true),
  );
  [10, 20, 640, 480, 320, 240].forEach((n, at) => view.setUint32(48 + 4 * at, n, true));
  if (length >= 76) view.setUint32(72, jpeg.length, true);
  all.set(jpeg, length);
  let at = length + jpeg.length;
  for (const [block, bytes] of blocks) {
    view.setUint8(at, block);
    view.setUint32(at + 4, bytes.length, true);
    all.set(bytes, at + 8);
    at += 8 + bytes.length;
  }
  return buffer;
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
