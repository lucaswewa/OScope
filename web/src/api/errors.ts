// What went wrong in a call to the server, as one model (ADR-0043). The
// server says it in four ways, each read here:
// - problem details (RFC 9457): a title, a status and a detail. A device's
//   refusal of a property's value has its kind as the title, and ADR-0012's
//   status (ADR-0034);
// - its checks' 422: `detail`, a list of what was refused, each with its
//   place (`loc`), a message and the check's name;
// - `{ "detail": "…" }`, for what isn't there;
// - an action's invocation, ended in `error` or `cancelled`: a device's
//   refusal leads its detail with its kind and status, "busy (409): …"
//   (ADR-0034).
// No answer is a failure too: the network, the time limit, or the caller's
// abort.

/** A device's kinds of refusal, each with its status (ADR-0012). */
export const REFUSALS = {
  unsupported: 400,
  "out of range": 400,
  unknown: 400,
  busy: 409,
  stopped: 409,
  alarm: 409,
  disconnected: 503,
  timeout: 504,
  device: 502,
} as const satisfies Record<string, number>;

/** Why a device refused. */
export type Refusal = keyof typeof REFUSALS;

/** How a call failed. */
export type Failure =
  /** A device refused: `refusal` says why. */
  | "refused"
  /** The server's checks refused the request: `fields` says what. */
  | "invalid"
  /** Nothing is there: 404. */
  | "missing"
  /** Not logged in, or no longer: 401. */
  | "unauthorised"
  /** Not allowed: 403. */
  | "forbidden"
  /** The action was cancelled. */
  | "cancelled"
  /** The action, or the job, ended in an error that isn't a device's refusal. */
  | "failed"
  /** Any other answer that isn't a success. */
  | "server"
  /** No answer: the network, or no server there. */
  | "unreachable"
  /** No answer within the time limit. */
  | "timeout"
  /** The caller stopped waiting. */
  | "aborted";

/** Something the server's checks refused in a request. */
export interface Field {
  /** Where, in the request's body: `["x"]`, or `[]` for the whole of it. */
  at: (string | number)[];
  message: string;
  /** The check's name: `float_parsing`, `missing`… */
  check: string;
}

/** What an ApiError says besides its failure and message. */
export interface ApiErrorParts {
  /** The answer's status, or the refusal's (ADR-0012). */
  status?: number;
  refusal?: Refusal;
  fields?: readonly Field[];
  /** The answer's body as the server gave it, for what reads it further. */
  body?: unknown;
  cause?: unknown;
}

/** A call to the server that failed. */
export class ApiError extends Error {
  override readonly name = "ApiError";
  readonly status: number | undefined;
  readonly refusal: Refusal | undefined;
  readonly fields: readonly Field[];
  readonly body: unknown;

  constructor(
    readonly failure: Failure,
    message: string,
    parts: ApiErrorParts = {},
  ) {
    super(message, parts.cause === undefined ? undefined : { cause: parts.cause });
    this.status = parts.status;
    this.refusal = parts.refusal;
    this.fields = parts.fields ?? [];
    this.body = parts.body;
  }
}

type Record_ = Record<string, unknown>;
const isRecord = (value: unknown): value is Record_ =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const isRefusal = (kind: unknown): kind is Refusal =>
  typeof kind === "string" && Object.hasOwn(REFUSALS, kind);
const textOf = (value: unknown): string | undefined =>
  typeof value === "string" && value !== "" ? value : undefined;

const BY_STATUS: Partial<Record<number, Failure>> = {
  401: "unauthorised",
  403: "forbidden",
  404: "missing",
};

/** One thing a 422 refused, from its `detail`. */
function fieldOf(issue: unknown): Field[] {
  if (!isRecord(issue) || typeof issue.msg !== "string") return [];
  const check = typeof issue.type === "string" ? issue.type : "";
  const loc = Array.isArray(issue.loc) ? issue.loc : [];
  const at = loc.filter((part): part is string | number =>
    ["string", "number"].includes(typeof part),
  );
  if (at[0] === "body") at.shift();
  // A body that isn't JSON is refused as a whole: its place is where the
  // parser stopped.
  return [{ at: check === "json_invalid" ? [] : at, message: issue.msg, check }];
}

/** An answer that isn't a success, as an ApiError. */
export function answerError(status: number, body: unknown, statusText = ""): ApiError {
  const detail = isRecord(body) ? body.detail : undefined;
  if (status === 422 && Array.isArray(detail)) {
    const fields = detail.flatMap(fieldOf);
    const message = fields
      .map((field) => (field.at.length > 0 ? `${field.at.join(".")}: ` : "") + field.message)
      .join("; ");
    return new ApiError("invalid", message || "the request was refused", { status, fields, body });
  }
  const title = isRecord(body) ? body.title : undefined;
  const message =
    textOf(detail) ??
    textOf(title) ??
    textOf(body) ??
    textOf(statusText) ??
    `the server answered ${status}`;
  if (isRefusal(title)) return new ApiError("refused", message, { status, refusal: title, body });
  return new ApiError(BY_STATUS[status] ?? "server", message, { status, body });
}

/** An invocation's end without its output: `error`, or `cancelled`. */
export function invocationError(invocation: { status: string; error?: unknown }): ApiError {
  const body = invocation.error;
  const detail = isRecord(body) ? textOf(body.detail) : undefined;
  if (invocation.status === "cancelled") {
    return new ApiError("cancelled", detail ?? "the action was cancelled", { body });
  }
  const head = /^([a-z][a-z ]*) \((\d{3})\): (.*)$/s.exec(detail ?? "");
  if (head && isRefusal(head[1])) {
    return new ApiError("refused", head[3]!, { status: Number(head[2]), refusal: head[1], body });
  }
  // A Thing's own refusal, such as the power actions' "conflict (409): …" (W4.3):
  // its status, and its words.
  if (head) return new ApiError("failed", head[3]!, { status: Number(head[2]), body });
  return new ApiError("failed", detail ?? "the action failed", { body });
}

/** A call that had no answer, given why it stopped if the caller knows. */
export function thrownError(thrown: unknown, why?: "timeout" | "aborted"): ApiError {
  if (thrown instanceof ApiError) return thrown;
  if (why === "timeout") {
    return new ApiError("timeout", "the server didn't answer in time", { cause: thrown });
  }
  if (why === "aborted") return new ApiError("aborted", "the call was stopped", { cause: thrown });
  const message = thrown instanceof Error ? thrown.message : String(thrown);
  return new ApiError("unreachable", message || "the server can't be reached", { cause: thrown });
}
