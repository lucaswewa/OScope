// The typed client (ADR-0043): calls typed by the server's description,
// each with a time limit and its caller's abort, and an answer that isn't a
// success thrown as an ApiError. The network is stood in for here; the
// contract tests (tests/e2e/contract.spec.ts) call a real server.
import { afterEach, describe, expect, it, vi } from "vitest";

import { call, fetched, pause, Server } from "./client.ts";

/** Stands in for the network: each request goes to `answer`; those made are kept. */
function network(answer: (request: Request) => Response | Promise<Response>): Request[] {
  const made: Request[] = [];
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    made.push(request);
    return answer(request);
  });
  return made;
}

/** No answer, until the request is aborted. */
const silent = (request: Request) =>
  new Promise<Response>((_, failed) => {
    request.signal.addEventListener("abort", () =>
      failed(new DOMException("aborted", "AbortError")),
    );
  });

const json = (value: unknown, status = 200, type = "application/json") =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": type } });

const elsewhere = new Server("http://127.0.0.1:8211/");
const version = (signal: AbortSignal) => elsewhere.api.GET("/api/v1/system/version", { signal });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a call", () => {
  it("is made relative to the page, by default, and gives the answer's value", async () => {
    const made = network(() => json("0.1.1"));
    const own = new Server();
    // A page served under a path of its own: relative to the page isn't to the root.
    history.pushState({}, "", "/oscope/");
    try {
      const version = await call((signal) => own.api.GET("/api/v1/system/version", { signal }));
      expect(version).toBe("0.1.1");
      expect(new URL(made[0]!.url).pathname).toBe("/oscope/api/v1/system/version");
    } finally {
      history.pushState({}, "", "/");
    }
  });

  it("is made to a server elsewhere when given its URL", async () => {
    const made = network(() => json("0.1.1"));
    await call(version);
    expect(made[0]!.url).toBe("http://127.0.0.1:8211/api/v1/system/version");
    expect(elsewhere.address("/api/v1/media/ping")).toBe("http://127.0.0.1:8211/api/v1/media/ping");
  });

  it("sends its path's parameters and its body as the description has them", async () => {
    const made = network(() => json(null));
    await call((signal) => elsewhere.api.PUT("/api/v1/camera/gain", { body: 12, signal }));
    const params = { path: { id: "an-id" } };
    await call((signal) =>
      elsewhere.api.GET("/api/v1/action_invocations/{id}", { params, signal }),
    );
    const said = made.map((request) => `${request.method} ${new URL(request.url).pathname}`);
    expect(said).toEqual(["PUT /api/v1/camera/gain", "GET /api/v1/action_invocations/an-id"]);
    expect(await made[0]!.json()).toBe(12);
  });

  it("throws an answer that isn't a success as an ApiError", async () => {
    network(() => json({ detail: "the device is busy", status: 409, title: "busy" }, 409));
    await expect(call(version)).rejects.toMatchObject({
      name: "ApiError",
      failure: "refused",
      refusal: "busy",
      status: 409,
      message: "the device is busy",
    });
  });

  it("stops at its time limit", async () => {
    network(silent);
    await expect(call(version, { timeout: 20 })).rejects.toMatchObject({ failure: "timeout" });
  });

  it("stops when its caller aborts, before its time limit", async () => {
    network(silent);
    const caller = new AbortController();
    const failed = call(version, { signal: caller.signal });
    caller.abort();
    await expect(failed).rejects.toMatchObject({ failure: "aborted" });
  });

  it("is unreachable when no answer can come", async () => {
    network(() => {
      throw new TypeError("Failed to fetch");
    });
    await expect(call(version)).rejects.toMatchObject({
      failure: "unreachable",
      message: "Failed to fetch",
    });
  });
});

describe("a route the description doesn't have", () => {
  const ping = elsewhere.address("/api/v1/media/ping");

  it("is fetched, and read as its caller asks", async () => {
    network(() => new Response("pong"));
    expect(await fetched(ping, (answer) => answer.text())).toBe("pong");
  });

  it("throws its failures as ApiErrors, its body read as JSON when it is", async () => {
    const lean = { title: "Bad Request", status: 400, detail: "no level 0 to slice" };
    network(() => json(lean, 400, "application/problem+json"));
    await expect(fetched(ping, (answer) => answer.text())).rejects.toMatchObject({
      failure: "server",
      status: 400,
      message: "no level 0 to slice",
    });
    network(() => new Response("plain words", { status: 503 }));
    await expect(fetched(ping, (answer) => answer.text())).rejects.toMatchObject({
      status: 503,
      message: "plain words",
    });
  });

  it("stops at its time limit, and when its caller aborts", async () => {
    network(silent);
    await expect(fetched(ping, (answer) => answer.text(), { timeout: 20 })).rejects.toMatchObject({
      failure: "timeout",
    });
    const caller = new AbortController();
    const failed = fetched(ping, (answer) => answer.text(), { signal: caller.signal });
    caller.abort();
    await expect(failed).rejects.toMatchObject({ failure: "aborted" });
  });

  it("sends what its caller adds to the request", async () => {
    const made = network(() => new Response("part", { status: 206 }));
    await fetched(ping, (answer) => answer.text(), {}, { headers: { range: "bytes=0-3" } });
    expect(made[0]!.headers.get("range")).toBe("bytes=0-3");
  });
});

describe("a pause between calls", () => {
  it("waits as long as it is asked", async () => {
    const from = performance.now();
    await pause(20);
    expect(performance.now() - from).toBeGreaterThanOrEqual(19);
  });

  it("ends when its caller aborts, or at once if it has", async () => {
    const caller = new AbortController();
    const waiting = pause(60_000, caller.signal);
    caller.abort();
    await expect(waiting).rejects.toMatchObject({ failure: "aborted" });
    await expect(pause(60_000, caller.signal)).rejects.toMatchObject({ failure: "aborted" });
  });
});
