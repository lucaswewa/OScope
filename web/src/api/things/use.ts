// Live Things in a component (W1.2, ADR-0044): a property, an event or an
// action, typed by its path as W1.1's calls are, and kept for as long as the
// component, or the effect scope, that asks for it lasts.
import { computed, onScopeDispose, shallowRef } from "vue";

import {
  follow,
  start,
  cancel as cancelling,
  type ActionPath,
  type Input,
  type Invocation,
  type Output,
} from "../actions.ts";
import type { paths, Server } from "../client.ts";
import { ApiError, invocationError } from "../errors.ts";
import { thingStore, type Told } from "./store.ts";

type Get<P extends keyof paths> = paths[P] extends { get: infer Operation } ? Operation : never;
type Put<P extends keyof paths> = paths[P] extends { put: infer Operation } ? Operation : never;
type Json<Content> = Content extends { content: { "application/json": infer T } } ? T : never;

/** The paths of Things' properties and events: `/api/v1/{thing}/{name}`. */
export type AffordancePath = {
  [P in keyof paths]: P extends `/api/v1/${string}/${infer Name}`
    ? Name extends "" | `${string}/${string}`
      ? never
      : P
    : never;
}[keyof paths];

/** What a property's GET gives. */
export type ValueOf<P extends AffordancePath> =
  Get<P> extends { responses: { 200: infer Answer } } ? Json<Answer> : unknown;

/** What a property's PUT takes: `never` for one that can't be written. */
export type WrittenOf<P extends AffordancePath> =
  Put<P> extends { requestBody: infer Body } ? Json<Body> : never;

/** A path's Thing, and its affordance's name. */
export function partsOf(path: string): [thing: string, name: string] {
  const found = /^\/api\/v1\/([^/]+)\/([^/]+)$/.exec(path);
  if (!found) throw new Error(`${path} isn't one of a Thing's affordances`);
  return [decodeURIComponent(found[1]!), decodeURIComponent(found[2]!)];
}

/**
 * A property, live: read at once, then kept by its Thing's WebSocket, or
 * read again every `every` ms and at each of its Thing's events if the
 * socket can't keep it.
 */
export function useProperty<P extends AffordancePath>(
  path: P,
  options: { every?: number; on?: Server } = {},
) {
  const [thing, name] = partsOf(path);
  const store = thingStore(thing, options.on);
  onScopeDispose(store.follow(name, options.every));
  const kept = () => store.kept.get(name);
  return {
    /** Its value: a write's, while the write is under way. */
    value: computed(() => {
      const property = kept();
      return (property?.writing ? property.writing.value : property?.value) as
        ValueOf<P> | undefined;
    }),
    /** Whether the server hasn't confirmed it for a few seconds. */
    stale: computed(() => store.isStale(name)),
    /** Why the server refused its last write. */
    refused: computed(() => kept()?.refused),
    /** Whether a write is under way. */
    writing: computed(() => kept()?.writing !== undefined),
    /** Writes it: shown at once, taken back if refused. */
    write: (value: WrittenOf<P>) => store.write(name, value),
  };
}

/** An event, live: each as its Thing tells it, the newest `keep` of them, in order. */
export function useEvent<T = unknown>(
  path: AffordancePath,
  options: { on?: Server; keep?: number } = {},
) {
  const [thing, name] = partsOf(path);
  const store = thingStore(thing, options.on);
  const told = shallowRef<(Told & { data: T })[]>([]);
  onScopeDispose(
    store.on(name, (one) => {
      told.value = [...told.value, one as Told & { data: T }].slice(-(options.keep ?? 50));
    }),
  );
  return { told, last: computed(() => told.value.at(-1)) };
}

const failed = new Set<(error: ApiError, path: string) => void>();

/** Failures that say themselves: a cancellation, an action left, an ended session, a lost server. */
const QUIET = new Set(["cancelled", "aborted", "unauthorised", "unreachable"]);

/**
 * Told each action run from a component that the server refused, or that
 * failed, with its error, whose message is the server's words: the shell
 * raises a notice of it (W2.1). Gives the means to stop.
 */
export function onActionFailed(listener: (error: ApiError, path: string) => void): () => void {
  failed.add(listener);
  return () => {
    failed.delete(listener);
  };
}

/**
 * An action, run from a component: its invocation as it goes, then its
 * output or its error. Leaving the component stops following it, and
 * doesn't cancel it.
 */
export function useAction<P extends ActionPath>(path: P, options: { on?: Server } = {}) {
  const invocation = shallowRef<Invocation>();
  const output = shallowRef<Output<P>>();
  const error = shallowRef<ApiError>();
  const running = computed(
    () =>
      invocation.value !== undefined &&
      !["completed", "error", "cancelled"].includes(invocation.value.status),
  );
  const leaving = new AbortController();
  onScopeDispose(() => leaving.abort());

  /** Runs it: its output, or undefined and `error` set. */
  async function run(input: Input<P>): Promise<Output<P> | undefined> {
    output.value = undefined;
    error.value = undefined;
    try {
      const started = await start(path, input, { on: options.on });
      invocation.value = started;
      const ended = await follow(started, {
        on: options.on,
        signal: leaving.signal,
        cancelling: false,
        seen: (now) => (invocation.value = now),
      });
      if (ended.status !== "completed") throw invocationError(ended);
      output.value = ended.output as Output<P>;
      return output.value;
    } catch (thrown) {
      error.value = thrown instanceof ApiError ? thrown : new ApiError("server", String(thrown));
      // Left, the component follows it no more: that isn't a failure.
      if (!leaving.signal.aborted && !QUIET.has(error.value.failure)) {
        for (const listener of failed) listener(error.value, path);
      }
      return undefined;
    }
  }

  /** Cancels it on the server, if it is running. */
  async function cancel(): Promise<void> {
    const now = invocation.value;
    if (now && running.value) await cancelling(now.id, { on: options.on });
  }

  return { run, cancel, running, invocation, output, error };
}
