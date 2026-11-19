// The preview's stream, in its worker (W1.5), on a socket stood in for: the
// first message under security; the parameters, whole, with each
// acknowledgement; a frame decoded and handed over; a JPEG that won't
// decode passed over; a version refused; and opening again.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FakeSocket, previewMessage } from "../../../tests/setup/stand-ins.ts";
import { KIND } from "./message.ts";
import {
  PreviewStream,
  REOPEN,
  type DecodedFrame,
  type FromWorker,
  type StreamSocket,
} from "./stream.ts";

const ASK = { width: 640, height: 480, quality: 70 };

/** A stream on stand-ins, and what it told the page. */
function streamOn(token?: string) {
  const posted: Array<{ message: FromWorker; transfer: Transferable[] | undefined }> = [];
  const decoded: Blob[] = [];
  const place = {
    clock: 1_000,
    failing: false,
    socket: (url: string) => new FakeSocket(url) as unknown as StreamSocket,
    decode: vi.fn(async (jpeg: Blob) => {
      decoded.push(jpeg);
      if (place.failing) throw new DOMException("not an image", "InvalidStateError");
      return { close: vi.fn() } as unknown as ImageBitmap;
    }),
    post: (message: FromWorker, transfer?: Transferable[]) => posted.push({ message, transfer }),
    now: () => place.clock,
  };
  const stream = new PreviewStream(place);
  stream.handle({
    kind: "open",
    url: "ws://127.0.0.1:8211/api/v1/preview/stream",
    token,
    ask: ASK,
  });
  return { stream, place, posted, decoded, socket: FakeSocket.last };
}

const frames = (posted: Array<{ message: FromWorker }>) =>
  posted.map(({ message }) => message).filter((m): m is DecodedFrame => m.kind === "frame");
const states = (posted: Array<{ message: FromWorker }>) =>
  posted.flatMap(({ message }) => (message.kind === "state" ? [message.state] : []));

describe("the preview's stream", () => {
  beforeEach(() => {
    FakeSocket.made = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("opens with its token first, then asks for everything, ready for message 0", () => {
    const { socket, posted } = streamOn("t1");
    expect([socket.url, socket.binaryType]).toEqual([
      "ws://127.0.0.1:8211/api/v1/preview/stream",
      "arraybuffer",
    ]);
    socket.opens();
    expect(socket.sent).toEqual([{ authorization: "Bearer t1" }, { ...ASK, ack: 0 }]);
    expect(states(posted)).toEqual(["connecting", "open"]);
  });

  it("asks at once without a token", () => {
    const { socket } = streamOn();
    socket.opens();
    expect(socket.sent).toEqual([{ ...ASK, ack: 0 }]);
  });

  it("hands each frame over decoded, and is ready for the next once it is drawn", async () => {
    const { stream, socket, posted, place, decoded } = streamOn();
    socket.opens();
    const stats = new TextEncoder().encode(JSON.stringify({ focus: 3 }));
    const message = previewMessage({ sequence: 0, blocks: [[KIND.stats, stats]] });
    place.clock = 1_234;
    socket.sendsBytes(message);
    await vi.waitFor(() => expect(frames(posted)).toHaveLength(1));
    const [frame] = frames(posted);
    expect([frame!.header.sequence, frame!.arrived, frame!.bytes, frame!.stats]).toEqual([
      0,
      1_234,
      message.byteLength,
      { focus: 3 },
    ]);
    expect(posted.at(-1)!.transfer).toEqual([frame!.bitmap]);
    expect(decoded.map((jpeg) => [jpeg.type, jpeg.size])).toEqual([["image/jpeg", 4]]);
    // Not acknowledged until the page has drawn it.
    expect(socket.sent).toEqual([{ ...ASK, ack: 0 }]);
    stream.handle({ kind: "drawn", sequence: 0 });
    stream.handle({ kind: "drawn", sequence: 0 });
    expect(socket.sent).toEqual([
      { ...ASK, ack: 0 },
      { ...ASK, ack: 1 },
    ]);
  });

  it("sends a new ask whole, with the acknowledgement it has", () => {
    const { stream, socket } = streamOn();
    socket.opens();
    stream.handle({ kind: "drawn", sequence: 4 });
    stream.handle({ kind: "ask", ask: { width: 320, height: 240 } });
    expect(socket.sent.at(-1)).toEqual({ width: 320, height: 240, ack: 5 });
  });

  it("passes over a JPEG that won't decode, as though it were drawn", async () => {
    const { socket, posted, place } = streamOn();
    socket.opens();
    place.failing = true;
    socket.sendsBytes(previewMessage({ sequence: 3 }));
    await vi.waitFor(() => expect(socket.sent.at(-1)).toEqual({ ...ASK, ack: 4 }));
    expect(frames(posted)).toEqual([]);
  });

  it("stops for good at a version this app doesn't know", async () => {
    const { socket, posted } = streamOn();
    socket.opens();
    socket.sendsBytes(previewMessage({ version: 2 }));
    await vi.waitFor(() => expect(states(posted).at(-1)).toBe("refused"));
    expect(posted.at(-1)!.message).toMatchObject({
      reason: "a preview of version 2: this app reads version 1",
    });
    expect(socket.readyState).toBe(FakeSocket.CLOSED);
    vi.advanceTimersByTime(REOPEN.most * 2);
    expect(FakeSocket.made).toHaveLength(1);
  });

  it("opens again after a wait that grows, and is short again once a socket settled", () => {
    const { socket, posted, place } = streamOn();
    socket.opens();
    socket.drops();
    expect(states(posted).at(-1)).toBe("closed");
    vi.advanceTimersByTime(REOPEN.first - 1);
    expect(FakeSocket.made).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.made).toHaveLength(2);
    // Refused as soon as it opens: twice the wait.
    FakeSocket.last.opens();
    FakeSocket.last.drops();
    vi.advanceTimersByTime(REOPEN.first * 2 - 1);
    expect(FakeSocket.made).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.made).toHaveLength(3);
    // Open long enough to have settled: the first wait again, and a new
    // session's messages, numbered from 0.
    FakeSocket.last.opens();
    place.clock += REOPEN.settled;
    FakeSocket.last.drops();
    vi.advanceTimersByTime(REOPEN.first);
    expect(FakeSocket.made).toHaveLength(4);
    FakeSocket.last.opens();
    expect(FakeSocket.last.sent).toEqual([{ ...ASK, ack: 0 }]);
  });

  it("closes when told, and doesn't open again", () => {
    const { stream, socket } = streamOn();
    socket.opens();
    stream.handle({ kind: "close" });
    expect(socket.readyState).toBe(FakeSocket.CLOSED);
    vi.advanceTimersByTime(REOPEN.most * 2);
    expect(FakeSocket.made).toHaveLength(1);
  });
});
