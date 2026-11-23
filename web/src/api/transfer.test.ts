// Uploads and downloads with their progress (W1.3): a download read as it
// comes, and an upload by XMLHttpRequest, each with the session's token.
// On a stand-in network and a stand-in XMLHttpRequest.
import { afterEach, describe, expect, it, vi } from "vitest";

import { standInNetwork } from "../../tests/setup/stand-ins.ts";
import { session } from "./client.ts";
import { receive, send, type Progress } from "./transfer.ts";

/** An XMLHttpRequest the test answers: the last one made, and what it was told. */
class FakeRequest extends EventTarget {
  static last: FakeRequest;
  readonly upload = new EventTarget();
  readonly headers: Record<string, string> = {};
  opened: [string, string] = ["", ""];
  sent: unknown;
  status = 0;
  statusText = "";
  responseText = "";
  timeout = 0;

  constructor() {
    super();
    FakeRequest.last = this;
  }
  open(method: string, address: string) {
    this.opened = [method, address];
  }
  setRequestHeader(name: string, value: string) {
    this.headers[name] = value;
  }
  send(body: unknown) {
    this.sent = body;
  }
  abort() {
    this.dispatchEvent(new Event("abort"));
  }
  /** The server's answer, after telling `steps` of the upload's progress. */
  answer(status: number, body: unknown, steps: [number, number][] = []) {
    for (const [loaded, total] of steps) {
      this.upload.dispatchEvent(
        Object.assign(new Event("progress"), { loaded, total, lengthComputable: true }),
      );
    }
    this.status = status;
    this.responseText = JSON.stringify(body);
    this.dispatchEvent(new Event("load"));
  }
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete session.token;
});

describe("a download", () => {
  it("is read as it comes, its progress told, with the session's token", async () => {
    session.token = "t1";
    const parts = [new Uint8Array([1, 2, 3]), new Uint8Array([4, 5, 6])];
    const made = standInNetwork({
      "GET /api/v1/data/p/a.bin": () =>
        new Response(
          new ReadableStream({
            start(controller) {
              for (const part of parts) controller.enqueue(part);
              controller.close();
            },
          }),
          { headers: { "content-length": "6", "content-type": "application/octet-stream" } },
        ),
    });
    const told: Progress[] = [];
    const blob = await receive("http://127.0.0.1:8211/api/v1/data/p/a.bin", {
      progress: (now) => told.push(now),
    });
    expect(told).toEqual([
      { done: 3, of: 6 },
      { done: 6, of: 6 },
    ]);
    expect([blob.size, blob.type]).toEqual([6, "application/octet-stream"]);
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([1, 2, 3, 4, 5, 6]);
    expect(made[0]!.headers.get("authorization")).toBe("Bearer t1");
  });
});

describe("an upload", () => {
  it("is sent with its type and the session's token, its progress told, and its answer given", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeRequest);
    session.token = "t1";
    const told: Progress[] = [];
    const body = new Blob(["123456"], { type: "image/tiff" });
    const sending = send("http://127.0.0.1:8211/api/v1/upload", body, {
      progress: (now) => told.push(now),
    });
    const request = FakeRequest.last;
    expect(request.opened).toEqual(["POST", "http://127.0.0.1:8211/api/v1/upload"]);
    expect(request.headers).toEqual({ authorization: "Bearer t1", "content-type": "image/tiff" });
    expect(request.sent).toBe(body);
    request.answer(200, { id: "a record" }, [
      [3, 6],
      [6, 6],
    ]);
    expect(await sending).toEqual({ id: "a record" });
    expect(told).toEqual([
      { done: 3, of: 6 },
      { done: 6, of: 6 },
    ]);
  });

  it("throws the server's refusal as an ApiError", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeRequest);
    const sending = send("http://127.0.0.1:8211/api/v1/upload", new Blob(["x"]));
    FakeRequest.last.answer(409, { status: 409, title: "busy", detail: "the store is busy" });
    await expect(sending).rejects.toMatchObject({ failure: "refused", refusal: "busy" });
  });

  it("stops when its caller aborts", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeRequest);
    const caller = new AbortController();
    const sending = send("http://127.0.0.1:8211/api/v1/upload", new Blob(["x"]), {
      signal: caller.signal,
    });
    caller.abort();
    await expect(sending).rejects.toMatchObject({ failure: "aborted" });
  });
});
