// A Thing's state (W1.2, ADR-0044): a Pinia store for each Thing in use,
// kept live. An observable property is kept by the Thing's WebSocket; any
// other is read at intervals, and again when the Thing tells an event. A
// write shows at once, and goes back if the server refuses it, with the
// reason. A value the server hasn't confirmed for a few seconds is stale.
import { defineStore } from "pinia";
import { reactive, shallowRef } from "vue";

import { call, server, type Server } from "../client.ts";
import { ApiError } from "../errors.ts";
import { description, observable } from "./descriptions.ts";
import { socketOf, type Heard } from "./socket.ts";

/** How long a value stays current without the server confirming it: 5 s. */
export const STALE_AFTER = 5_000;

/** How often a property its socket doesn't keep is read, unless told: 1 s. */
export const EVERY = 1_000;

/** A property's state. */
export interface Kept {
  /** Its value, as the server last told it. */
  value: unknown;
  /** When the server last confirmed it, on `performance.now()`'s clock: 0 before. */
  at: number;
  /** Whether the Thing's WebSocket keeps it. */
  observed: boolean;
  /** A write under way: shown in the value's place until it is taken or refused. */
  writing?: { value: unknown } | undefined;
  /** Why the server refused the last write. */
  refused?: ApiError | undefined;
  /** Why the last read failed. */
  failed?: ApiError | undefined;
}

/** An event, as its Thing told it. */
export interface Told {
  data: unknown;
  time: string;
}

/** What a call through openapi-fetch answers. */
type Answered = Promise<{ data?: unknown; error?: unknown; response: Response }>;

/** A Thing's store, on a server: one each. */
export function thingStore(thing: string, on: Server = server) {
  const id = on === server ? `thing:${thing}` : `thing:${on.base}:${thing}`;
  return defineStore(id, () => {
    const socket = socketOf(thing, on);
    const kept = reactive(new Map<string, Kept>());
    /** The store's clock, which a value's staleness is read against: it ticks each second in use. */
    const now = shallowRef(performance.now());
    const followed = new Map<string, { count: number; every: number; stop: () => void }>();
    const listeners = new Map<string, Set<(told: Told) => void>>();
    let users = 0;
    let unlisten: (() => void) | undefined;
    let ticking: ReturnType<typeof setInterval> | undefined;

    // A property's path is known only at run time: it is checked by the composables' types.
    const path = (name: string) =>
      `/api/v1/${encodeURIComponent(thing)}/${encodeURIComponent(name)}`;
    const get = on.api.GET as unknown as (path: string, init: { signal: AbortSignal }) => Answered;
    const put = on.api.PUT as unknown as (
      path: string,
      init: { body: unknown; signal: AbortSignal },
    ) => Answered;

    function entry(name: string): Kept {
      if (!kept.has(name)) kept.set(name, { value: undefined, at: 0, observed: false });
      return kept.get(name)!;
    }

    async function read(name: string): Promise<void> {
      const property = entry(name);
      try {
        const value: unknown = await call((signal) => get(path(name), { signal }));
        property.value = value;
        property.at = performance.now();
        property.failed = undefined;
      } catch (thrown) {
        property.failed = thrown instanceof ApiError ? thrown : undefined;
      }
    }

    function hear(heard: Heard): void {
      if (heard.kind === "property") {
        const property = kept.get(heard.name);
        if (property) {
          property.value = heard.value;
          property.at = performance.now();
        }
      } else if (heard.kind === "event") {
        for (const listener of listeners.get(heard.name) ?? []) {
          listener({ data: heard.data, time: heard.time });
        }
        // What the socket doesn't keep may have changed with it.
        for (const [name, property] of kept) {
          if (!property.observed && followed.has(name)) void read(name);
        }
      }
    }

    function tick(): void {
      now.value = performance.now();
      // An open socket confirms what it keeps: it would have told a change.
      for (const property of kept.values()) {
        if (property.observed && socket.open.value) property.at = now.value;
      }
    }

    /** The Thing's socket and the clock, from the first use to the last. */
    function use(): () => void {
      if (users++ === 0) {
        unlisten = socket.listen(hear);
        ticking = setInterval(tick, 1_000);
      }
      let used = true;
      return () => {
        if (!used) return;
        used = false;
        if (--users === 0) {
          unlisten?.();
          clearInterval(ticking);
        }
      };
    }

    /** Starts keeping a property, the socket's way or by reading it. */
    function keep(name: string, every: number): () => void {
      let stopped = false;
      let timer: ReturnType<typeof setInterval> | undefined;
      void read(name);
      void description(thing, on)
        .then((described) => {
          if (stopped) return;
          const property = described.properties[name];
          if (property && observable(property)) {
            entry(name).observed = true;
            socket.observe(name);
          } else {
            timer = setInterval(() => void read(name), every);
          }
          // Each of the Thing's events says that what isn't observed may have changed.
          for (const event of Object.keys(described.events)) socket.subscribe(event);
        })
        .catch(() => {
          // Without the descriptions, it is read.
          if (!stopped) timer = setInterval(() => void read(name), every);
        });
      return () => {
        stopped = true;
        clearInterval(timer);
      };
    }

    /** Follows a property: kept live from now until the function returned is called. */
    function follow(name: string, every = EVERY): () => void {
      const done = use();
      const already = followed.get(name);
      if (!already) {
        followed.set(name, { count: 1, every, stop: keep(name, every) });
      } else {
        already.count++;
        // The most often anyone asked for it.
        if (every < already.every && !entry(name).observed) {
          already.stop();
          Object.assign(already, { every, stop: keep(name, every) });
        }
      }
      let following = true;
      return () => {
        if (!following) return;
        following = false;
        const one = followed.get(name)!;
        if (--one.count === 0) {
          one.stop();
          followed.delete(name);
        }
        done();
      };
    }

    /** Listens to an event: told each, from now until the function returned is called. */
    function on_(event: string, listener: (told: Told) => void): () => void {
      const done = use();
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event)!.add(listener);
      socket.subscribe(event);
      return () => {
        listeners.get(event)?.delete(listener);
        done();
      };
    }

    /**
     * Writes a property. The value shows at once; taken, it is read again,
     * for what the device made of it; refused, the last value comes back,
     * with the server's reason in `refused`.
     */
    async function write(name: string, value: unknown): Promise<void> {
      const property = entry(name);
      property.writing = { value };
      property.refused = undefined;
      try {
        await call((signal) => put(path(name), { body: value, signal }));
        property.value = value;
        property.at = performance.now();
        if (!property.observed) await read(name);
      } catch (thrown) {
        property.refused =
          thrown instanceof ApiError ? thrown : new ApiError("server", String(thrown));
      } finally {
        property.writing = undefined;
      }
    }

    /** Whether a property's value is stale: not confirmed by the server for `STALE_AFTER`. */
    function isStale(name: string): boolean {
      const property = kept.get(name);
      return property !== undefined && property.at > 0 && now.value - property.at > STALE_AFTER;
    }

    return { kept, now, open: socket.open, follow, on: on_, write, isStale, read };
  })();
}
