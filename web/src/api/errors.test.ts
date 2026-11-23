// The error model (ADR-0043), from the server's own answers, recorded by
// scripts/record-answers.mjs: each read as the failure it is.
import { describe, expect, it } from "vitest";

import { answerError, ApiError, invocationError, REFUSALS, thrownError } from "./errors.ts";
import recorded from "./recorded-answers.json";

type Name = keyof typeof recorded;
const answered = (name: Name) => recorded[name] as unknown as { status: number; body: unknown };
const ended = (name: Name) => recorded[name] as unknown as { status: string; error: unknown };
const fromAnswer = (name: Name) => answerError(answered(name).status, answered(name).body);

describe("an answer that isn't a success", () => {
  it("is the checks' refusal, naming each field from the body's root, in a 422", () => {
    const error = fromAnswer("invalid");
    expect(error).toBeInstanceOf(ApiError);
    expect([error.failure, error.status]).toEqual(["invalid", 422]);
    const number = "Input should be a valid number, unable to parse string as a number";
    expect(error.fields).toEqual([
      { at: ["x"], message: number, check: "float_parsing" },
      { at: ["y"], message: "Field required", check: "missing" },
      { at: ["z"], message: "Field required", check: "missing" },
    ]);
    expect(error.message).toBe(`x: ${number}; y: Field required; z: Field required`);
    // As the server gave it, for a form's refusals (W0.6).
    expect(error.body).toBe(answered("invalid").body);
  });

  it("refuses a body that isn't JSON as a whole, not where its parser stopped", () => {
    const error = fromAnswer("malformed");
    expect(error.failure).toBe("invalid");
    expect(error.fields).toEqual([{ at: [], message: "JSON decode error", check: "json_invalid" }]);
  });

  it("refuses as a whole a value the action couldn't read", () => {
    const error = fromAnswer("unreadable");
    expect(error.fields).toEqual([
      {
        at: [],
        message: "Value error, UUID parsing failed: invalid character: found `n` at 0",
        check: "value_error",
      },
    ]);
  });

  it("is missing in a 404, in the server's words", () => {
    const error = fromAnswer("missing");
    expect([error.failure, error.status]).toEqual(["missing", 404]);
    expect(error.message).toBe(
      "No action invocation found with ID 00000000-0000-0000-0000-000000000000",
    );
  });

  it("is a device's refusal of a value, its kind the title (ADR-0034)", () => {
    const range = fromAnswer("outOfRange");
    expect([range.failure, range.refusal, range.status]).toEqual(["refused", "out of range", 400]);
    expect(range.message).toBe("the gain must be from 0 to 240, and was -5");
    const busy = fromAnswer("busy");
    expect([busy.failure, busy.refusal, busy.status]).toEqual(["refused", "busy", 409]);
    expect(busy.message).toMatch(/^the device is busy: /);
  });

  it("reads the media plane's leaner problem details alike", () => {
    const error = fromAnswer("noLevel");
    expect([error.failure, error.status, error.refusal]).toEqual(["server", 400, undefined]);
    expect(error.message).toBe("no level 0 to slice");
  });

  it.each([
    [401, "unauthorised"],
    [403, "forbidden"],
    [404, "missing"],
    [400, "server"],
    [500, "server"],
  ] as const)("is %i: %s", (status, failure) => {
    expect(answerError(status, { detail: "why" }).failure).toBe(failure);
  });

  it("says its status, or the status's text, when it says nothing else", () => {
    expect(answerError(502, "", "Bad Gateway").message).toBe("Bad Gateway");
    expect(answerError(500, undefined).message).toBe("the server answered 500");
    expect(answerError(500, "a plain text").message).toBe("a plain text");
    expect(answerError(409, { title: "a title alone" }).message).toBe("a title alone");
  });

  it("isn't a refusal whose kind the device's refusals don't know", () => {
    const error = answerError(409, { title: "toString", detail: "a trap" });
    expect([error.failure, error.refusal]).toEqual(["server", undefined]);
  });
});

describe("a device's refusals", () => {
  it("have ADR-0012's statuses", () => {
    expect(REFUSALS).toEqual({
      unsupported: 400,
      "out of range": 400,
      unknown: 400,
      busy: 409,
      stopped: 409,
      alarm: 409,
      disconnected: 503,
      timeout: 504,
      device: 502,
    });
  });
});

describe("an invocation that ended without its output", () => {
  it("is a device's refusal when one leads its detail: kind, status and words", () => {
    const alarm = invocationError(ended("alarm"));
    expect([alarm.failure, alarm.refusal, alarm.status]).toEqual(["refused", "alarm", 409]);
    expect(alarm.message).toBe("the stage is in alarm: homing required");
    const beyond = invocationError(ended("beyond"));
    expect([beyond.failure, beyond.refusal, beyond.status]).toEqual([
      "refused",
      "out of range",
      400,
    ]);
    expect(beyond.message).toBe("the x target in µm must be from 0 to 100000, and was 1000000000");
  });

  it("is cancelled when it was", () => {
    const error = invocationError(ended("cancelled"));
    expect([error.failure, error.message]).toEqual(["cancelled", "The action was cancelled."]);
  });

  it("is a failure in its own words when no refusal leads it", () => {
    const error = invocationError({ status: "error", error: { detail: "the pipeline broke" } });
    expect([error.failure, error.message, error.status]).toEqual([
      "failed",
      "the pipeline broke",
      undefined,
    ]);
    expect(invocationError({ status: "error", error: null }).message).toBe("the action failed");
  });

  it("is a failure with its status, in its words, when a Thing's own refusal leads it", () => {
    const detail = "conflict (409): a burst is running on the instrument";
    const error = invocationError({ status: "error", error: { detail } });
    expect([error.failure, error.message, error.status, error.refusal]).toEqual([
      "failed",
      "a burst is running on the instrument",
      409,
      undefined,
    ]);
  });
});

describe("a call with no answer", () => {
  it("timed out when the time limit stopped it", () => {
    const error = thrownError(new DOMException("aborted", "AbortError"), "timeout");
    expect([error.failure, error.message]).toEqual(["timeout", "the server didn't answer in time"]);
    expect(error.cause).toBeInstanceOf(DOMException);
  });

  it("aborted when its caller stopped it", () => {
    expect(thrownError(new DOMException("aborted", "AbortError"), "aborted").failure).toBe(
      "aborted",
    );
  });

  it("is otherwise unreachable, in the network's words", () => {
    const error = thrownError(new TypeError("Failed to fetch"));
    expect([error.failure, error.message]).toEqual(["unreachable", "Failed to fetch"]);
  });

  it("passes an ApiError on as it is", () => {
    const refused = new ApiError("refused", "busy", { refusal: "busy", status: 409 });
    expect(thrownError(refused, "timeout")).toBe(refused);
  });
});
