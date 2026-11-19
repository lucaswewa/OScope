// A Thing's WebSocket (W1.2, ADR-0044): `/api/v1/{thing}/ws`, one a Thing
// in use, shared by all that follow it. It observes the properties and
// subscribes to the events it is asked to, and asks again each time it
// opens. Closed, it opens again, waiting longer each time. Under security,
// its first message is the session's token (ADR-0031).
import { shallowRef } from "vue";

import { server, session, type Server } from "../client.ts";

/** What a Thing's socket hears. */
export type Heard =
  | { kind: "property"; name: string; value: unknown }
  | { kind: "event"; name: string; data: unknown; time: string }
  | { kind: "refused"; name: string; operation: string; status: number; detail: string };

/**
 * After it closes: 250 ms, then twice as long each time, up to 10 s; 250 ms
 * again once it had stayed open for 5 s.
 */
export const REOPEN = { first: 250, growth: 2, most: 10_000, settled: 5_000 };

type Raw = Record<string, unknown>;
const isRaw = (value: unknown): value is Raw =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** What a message says: a property's value or an event for each name it holds, or a refusal. */
export function heardIn(text: string): Heard[] {
  let message: unknown;
  try {
    message = JSON.parse(text);
  } catch {
    return [];
  }
  if (!isRaw(message)) return [];
  const data = isRaw(message.data) ? message.data : {};
  switch (message.messageType) {
    case "propertyStatus":
      return Object.entries(data).map(([name, value]) => ({ kind: "property", name, value }));
    case "event":
      return Object.entries(data).flatMap(([name, event]): Heard[] =>
        isRaw(event)
          ? [{ kind: "event", name, data: event.data, time: String(event.timestamp) }]
          : [],
      );
    case "response": {
      const error = isRaw(message.error) ? message.error : {};
      return [
        {
          kind: "refused",
          name: String(message.name),
          operation: String(message.operation),
          status: Number(error.status),
          detail: String(error.detail ?? ""),
        },
      ];
    }
    default:
      return [];
  }
}

type Asking = "addPropertyObservation" | "addEventSubscription";

/** A Thing's WebSocket, shared. */
export class ThingSocket {
  /** Whether it is open. */
  readonly open = shallowRef(false);
  /** When it last opened, or heard something: `performance.now()`'s time, 0 before. */
  readonly heard = shallowRef(0);
  private socket: WebSocket | undefined;
  private readonly asked: Record<Asking, Set<string>> = {
    addPropertyObservation: new Set(),
    addEventSubscription: new Set(),
  };
  private readonly listeners = new Set<(heard: Heard) => void>();
  private wait = REOPEN.first;
  private opened = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    readonly thing: string,
    private readonly on: Server = server,
  ) {}

  /** Listens to it, opening it if it isn't: the function returned stops, and the last to stop closes it. */
  listen(listener: (heard: Heard) => void): () => void {
    this.listeners.add(listener);
    if (this.listeners.size === 1) this.connect();
    return () => {
      if (this.listeners.delete(listener) && this.listeners.size === 0) this.disconnect();
    };
  }

  /** Observes a property, now and each time it opens. */
  observe(property: string): void {
    this.ask("addPropertyObservation", property);
  }

  /** Subscribes to an event, now and each time it opens. */
  subscribe(event: string): void {
    this.ask("addEventSubscription", event);
  }

  private ask(asking: Asking, name: string): void {
    if (this.asked[asking].has(name)) return;
    this.asked[asking].add(name);
    this.send(asking, [name]);
  }

  private send(asking: Asking, names: string[]): void {
    if (names.length === 0 || this.socket?.readyState !== WebSocket.OPEN) return;
    const data = Object.fromEntries(names.map((name) => [name, {}]));
    this.socket.send(JSON.stringify({ messageType: asking, data }));
  }

  private connect(): void {
    clearTimeout(this.timer);
    const socket = new WebSocket(
      this.on.socketAddress(`/api/v1/${encodeURIComponent(this.thing)}/ws`),
    );
    this.socket = socket;
    socket.addEventListener("open", () => {
      if (session.token) socket.send(JSON.stringify({ authorization: `Bearer ${session.token}` }));
      this.opened = performance.now();
      this.heard.value = this.opened;
      this.open.value = true;
      for (const asking of ["addPropertyObservation", "addEventSubscription"] as const) {
        this.send(asking, [...this.asked[asking]]);
      }
    });
    socket.addEventListener("message", (event: MessageEvent) => {
      this.heard.value = performance.now();
      if (typeof event.data !== "string") return;
      for (const heard of heardIn(event.data)) {
        for (const listener of this.listeners) listener(heard);
      }
    });
    socket.addEventListener("close", () => {
      // Closed by its last listener: it stays closed.
      if (this.socket !== socket) return;
      this.socket = undefined;
      this.open.value = false;
      if (this.opened > 0 && performance.now() - this.opened >= REOPEN.settled) {
        this.wait = REOPEN.first;
      }
      this.opened = 0;
      this.timer = setTimeout(() => this.connect(), this.wait);
      this.wait = Math.min(REOPEN.most, this.wait * REOPEN.growth);
    });
  }

  private disconnect(): void {
    clearTimeout(this.timer);
    const socket = this.socket;
    this.socket = undefined;
    socket?.close();
    this.open.value = false;
  }
}

const sockets = new WeakMap<Server, Map<string, ThingSocket>>();

/** A Thing's socket on a server: one each, shared. */
export function socketOf(thing: string, on: Server = server): ThingSocket {
  let things = sockets.get(on);
  if (!things) sockets.set(on, (things = new Map()));
  let socket = things.get(thing);
  if (!socket) things.set(thing, (socket = new ThingSocket(thing, on)));
  return socket;
}
