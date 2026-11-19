import { describe, expect, it } from "vitest";

import { requestsToTell } from "./leaseRequests";

describe("another's request for the lease", () => {
  it("is told once, while this app holds the lease", () => {
    const told = new Set<string>();
    expect(requestsToTell(told, true, ["ann"])).toEqual({ fresh: ["ann"], gone: [] });
    expect(requestsToTell(told, true, ["ann"])).toEqual({ fresh: [], gone: [] });
    expect(requestsToTell(told, true, ["ann", "bo"])).toEqual({ fresh: ["bo"], gone: [] });
  });

  it("has gone once its asker stops asking, and is told again if it asks again", () => {
    const told = new Set<string>();
    requestsToTell(told, true, ["ann", "bo"]);
    expect(requestsToTell(told, true, ["bo"])).toEqual({ fresh: [], gone: ["ann"] });
    expect(requestsToTell(told, true, ["bo", "ann"])).toEqual({ fresh: ["ann"], gone: [] });
  });

  it("isn't told while another holds the lease, and every one told has gone", () => {
    const told = new Set<string>();
    expect(requestsToTell(told, false, ["ann"])).toEqual({ fresh: [], gone: [] });
    requestsToTell(told, true, ["ann", "bo"]);
    expect(requestsToTell(told, false, ["ann", "bo"])).toEqual({ fresh: [], gone: ["ann", "bo"] });
    expect(told.size).toBe(0);
    // Held again, the request still standing: told again.
    expect(requestsToTell(told, true, ["ann"])).toEqual({ fresh: ["ann"], gone: [] });
  });
});
