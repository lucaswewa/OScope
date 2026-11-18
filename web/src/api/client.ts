// The typed client (ADR-0043): calls to the server, typed from its own
// description of its API (src/api/generated/openapi.ts), made over fetch by
// openapi-fetch. Each call has a time limit, and stops when its caller
// aborts; an answer that isn't a success, or none, is thrown as an ApiError.
import createClient, { type Client } from "openapi-fetch";

import { answerError, ApiError, thrownError } from "./errors.ts";
import type { components, paths } from "./generated/openapi.ts";

export type { components, paths };

/** One of the server's schemas, by its name. */
export type Schema<Name extends keyof components["schemas"]> = components["schemas"][Name];

/** How long a call waits for its answer, unless told otherwise: 10 s. */
export const TIME_LIMIT = 10_000;

/** What bounds a call. */
export interface Limits {
  /** The caller's: when it aborts, the call stops. */
  signal?: AbortSignal | undefined;
  /** How long to wait for the answer, in ms. */
  timeout?: number | undefined;
}

/** A server: its typed client, and the addresses of its other routes. */
export class Server {
  readonly api: Client<paths>;

  /**
   * @param base where it is: `.`, the server that serves the app, its
   *   addresses relative to the page; or its URL.
   */
  constructor(readonly base = ".") {
    // fetch is looked up at each call, so that a test can stand in for it.
    this.api = createClient<paths>({ baseUrl: base, fetch: (request) => fetch(request) });
  }

  /** The address of one of its paths, `/api/v1/…`. */
  address(path: string): string {
    return this.base.replace(/\/+$/, "") + path;
  }
}

/** The server that serves the app. */
export const server = new Server();

/** Waits `ms` between calls, unless `signal` aborts first. */
export function pause(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((done, failed) => {
    const stop = () => {
      clearTimeout(timer);
      failed(new ApiError("aborted", "the call was stopped"));
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", stop);
      done();
    }, ms);
    if (signal?.aborted) stop();
    else signal?.addEventListener("abort", stop, { once: true });
  });
}

/** The signal a call is made with, and why it ended, if it did. */
function bounded(limits: Limits) {
  const timer = AbortSignal.timeout(limits.timeout ?? TIME_LIMIT);
  const signal = limits.signal ? AbortSignal.any([limits.signal, timer]) : timer;
  const why = () => (limits.signal?.aborted ? "aborted" : timer.aborted ? "timeout" : undefined);
  return { signal, why };
}

/** What openapi-fetch answers a call with. */
interface Answered<T> {
  data?: T;
  error?: unknown;
  response: Response;
}

/**
 * A call's value. `start` makes the call with the signal it is given, which
 * ends it at the time limit or when the caller aborts.
 */
export async function call<T>(
  start: (signal: AbortSignal) => Promise<Answered<T>>,
  limits: Limits = {},
): Promise<T> {
  const { signal, why } = bounded(limits);
  let answered: Answered<T>;
  try {
    answered = await start(signal);
  } catch (thrown) {
    throw thrownError(thrown, why());
  }
  const { data, error, response } = answered;
  if (!response.ok) throw answerError(response.status, error, response.statusText);
  return data as T;
}

/** An answer's body: its JSON, or its text. */
async function bodyOf(response: Response): Promise<unknown> {
  const text = await response.text();
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

/**
 * A route outside the OpenAPI document (the media plane, ADR-0006), with
 * the same limits and errors as `call`: `read` takes what it needs from a
 * successful answer.
 */
export async function fetched<T>(
  address: string,
  read: (response: Response) => Promise<T>,
  limits: Limits = {},
  init: RequestInit = {},
): Promise<T> {
  const { signal, why } = bounded(limits);
  try {
    const response = await fetch(address, { ...init, signal });
    if (!response.ok) {
      throw answerError(response.status, await bodyOf(response), response.statusText);
    }
    return await read(response);
  } catch (thrown) {
    throw thrownError(thrown, why());
  }
}
