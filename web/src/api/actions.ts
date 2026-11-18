// Actions (ADR-0043): started, then followed to their end through
// `action_invocations/{id}`, or cancelled there. An action answers its start
// at once, 201 with its invocation, and carries on; its output, or its
// error, comes with the invocation once it has ended.
import {
  call,
  pause,
  server,
  type Limits,
  type paths,
  type Schema,
  type Server,
} from "./client.ts";
import { invocationError } from "./errors.ts";

/** An action's invocation: where it is in its life, and how it ended. */
export type Invocation = Schema<"Invocation">;

type Post<P extends keyof paths> = paths[P] extends { post: infer Operation } ? Operation : never;
type Json<Content> = Content extends { content: { "application/json": infer T } } ? T : never;

/** The paths of actions: those that answer a POST with 201, an invocation. */
export type ActionPath = {
  [P in keyof paths]: Post<P> extends { responses: { 201: unknown } } ? P : never;
}[keyof paths];

/** What an action takes: nothing, too, for one whose input is optional. */
export type Input<P extends ActionPath> =
  Post<P> extends { requestBody: infer Body }
    ? Json<Body>
    : Post<P> extends { requestBody?: infer Body }
      ? Json<NonNullable<Body>> | undefined
      : undefined;

/** What an action gives when it has completed. */
export type Output<P extends ActionPath> =
  Post<P> extends { responses: { 200: infer Answer } } ? Json<Answer> : unknown;

/** How an action is followed. */
export interface Following extends Limits {
  /** The server, if not the app's. */
  on?: Server | undefined;
  /** Told the invocation as it starts, and each time its status changes. */
  seen?: (invocation: Invocation) => void;
}

const ENDED = new Set<string>(["completed", "error", "cancelled"]);

/** Its first look 50 ms after the start, each wait then half as long again, to 1 s. */
const PAUSE = { first: 50, growth: 1.5, most: 1000 };

/** Starts an action: its invocation, which may not have run yet. */
export async function start<P extends ActionPath>(
  path: P,
  input: Input<P>,
  options: Limits & { on?: Server | undefined } = {},
): Promise<Invocation> {
  const on = options.on ?? server;
  // openapi-fetch's POST is typed by its path; `path` is any action's here,
  // so its input is checked by this function's signature instead.
  const post = on.api.POST as unknown as (
    path: string,
    init: { body: unknown; signal: AbortSignal },
  ) => Promise<{ data?: Invocation; error?: unknown; response: Response }>;
  return call((signal) => post(path, { body: input ?? {}, signal }), options);
}

/** Cancels an invocation; it stops at its action's next check. */
export async function cancel(
  id: string,
  options: Limits & { on?: Server | undefined } = {},
): Promise<void> {
  const on = options.on ?? server;
  await call(
    (signal) =>
      on.api.DELETE("/api/v1/action_invocations/{id}", { params: { path: { id } }, signal }),
    options,
  );
}

/**
 * Follows an invocation to its end: completed, in error or cancelled. If
 * the caller aborts first, the invocation is cancelled on the server too.
 */
export async function follow(started: Invocation, options: Following = {}): Promise<Invocation> {
  const on = options.on ?? server;
  const limits: Limits = { timeout: options.timeout };
  if (options.signal) limits.signal = options.signal;
  let invocation = started;
  let wait = PAUSE.first;
  options.seen?.(invocation);
  try {
    while (!ENDED.has(invocation.status)) {
      await pause(wait, options.signal);
      wait = Math.min(PAUSE.most, wait * PAUSE.growth);
      const id = invocation.id;
      const now = await call(
        (signal) =>
          on.api.GET("/api/v1/action_invocations/{id}", { params: { path: { id } }, signal }),
        limits,
      );
      if (now.status !== invocation.status) options.seen?.(now);
      invocation = now;
    }
  } catch (thrown) {
    if (options.signal?.aborted && !ENDED.has(invocation.status)) {
      // Not with the caller's signal, which has aborted: on a time limit of its own.
      await cancel(invocation.id, { on }).catch(() => undefined);
    }
    throw thrown;
  }
  return invocation;
}

/**
 * Starts an action and follows it to its end: its output once it has
 * completed. An error, or a cancellation, is thrown as an ApiError.
 */
export async function act<P extends ActionPath>(
  path: P,
  input: Input<P>,
  options: Following = {},
): Promise<Output<P>> {
  const ended = await follow(await start(path, input, options), options);
  if (ended.status !== "completed") throw invocationError(ended);
  return ended.output as Output<P>;
}
