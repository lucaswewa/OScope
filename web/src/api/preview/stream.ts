// The preview's stream, in its worker (W1.5, ADR-0047): its WebSocket, with
// the first message under security; its parameters, sent whole with the
// acknowledgement each time; each message read, and its JPEG decoded off
// the page's thread; and the socket opened again, after a growing wait,
// when it closes. A version this app doesn't know stops it for good.
import { readMessage, type LiveStats, type PreviewHeader, type SaturationMask } from "./message.ts";
import type { PreviewAsk } from "./params.ts";

/** What the page tells the worker. */
export type ToWorker =
  | { kind: "open"; url: string; token?: string | undefined; ask: PreviewAsk }
  | { kind: "ask"; ask: PreviewAsk }
  /** The page drew this message: the stream is ready for the next. */
  | { kind: "drawn"; sequence: number }
  | { kind: "close" };

/** A frame, decoded, as the page gets it. */
export interface DecodedFrame {
  kind: "frame";
  header: PreviewHeader;
  bitmap: ImageBitmap;
  /** When it arrived, in milliseconds since the epoch, on the page's clock. */
  arrived: number;
  /** Its message's size, in bytes. */
  bytes: number;
  stats?: LiveStats | undefined;
  mask?: SaturationMask | undefined;
}

/** How the stream stands. */
export type StreamState = "connecting" | "open" | "closed" | "refused";

/** What the worker tells the page. */
export type FromWorker = DecodedFrame | { kind: "state"; state: StreamState; reason?: string };

/** The socket's part the stream uses: a browser's WebSocket, or a test's. */
export interface StreamSocket extends EventTarget {
  binaryType: BinaryType;
  readonly readyState: number;
  send(data: string): void;
  close(): void;
}

/** What the stream is given: a worker's own, or a test's. */
export interface StreamPlace {
  socket(url: string): StreamSocket;
  decode(jpeg: Blob): Promise<ImageBitmap>;
  post(message: FromWorker, transfer?: Transferable[]): void;
  /** Milliseconds since the epoch: a worker's `performance.now()` has another origin than the page's. */
  now(): number;
}

/**
 * The waits before opening again, in milliseconds: from the first, doubling,
 * to the most; the first again once a socket stayed open long enough to
 * have settled.
 */
export const REOPEN = { first: 250, most: 10_000, settled: 5_000 };

const OPEN = 1;

/** One page's stream. */
export class PreviewStream {
  private socket: StreamSocket | undefined;
  private url = "";
  private token: string | undefined;
  private ask: PreviewAsk = { width: 0, height: 0 };
  /** The next message the page is ready for: one more than the last it drew. */
  private ready = 0;
  private wait = REOPEN.first;
  private opened: number | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private wanted = false;

  constructor(private readonly place: StreamPlace) {}

  /** Does what the page says. */
  handle(message: ToWorker): void {
    switch (message.kind) {
      case "open":
        ({ url: this.url, token: this.token, ask: this.ask } = message);
        this.wanted = true;
        this.connect();
        break;
      case "ask":
        this.ask = message.ask;
        this.send();
        break;
      case "drawn":
        this.readyFor(message.sequence + 1);
        break;
      case "close":
        this.stop();
        break;
    }
  }

  private connect(): void {
    clearTimeout(this.timer);
    const socket = this.place.socket(this.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    // A new session numbers its messages from 0.
    this.ready = 0;
    this.place.post({ kind: "state", state: "connecting" });
    socket.addEventListener("open", () => {
      if (this.token) socket.send(JSON.stringify({ authorization: `Bearer ${this.token}` }));
      this.opened = this.place.now();
      this.place.post({ kind: "state", state: "open" });
      this.send();
    });
    socket.addEventListener("message", (event) => {
      void this.receive((event as MessageEvent).data);
    });
    socket.addEventListener("close", () => {
      if (this.socket !== socket) return;
      this.socket = undefined;
      if (!this.wanted) return;
      this.place.post({ kind: "state", state: "closed" });
      // A socket refused as soon as it opens isn't settled: the wait grows.
      const stayed = this.opened === undefined ? 0 : this.place.now() - this.opened;
      if (stayed >= REOPEN.settled) this.wait = REOPEN.first;
      this.opened = undefined;
      this.timer = setTimeout(() => this.connect(), this.wait);
      this.wait = Math.min(this.wait * 2, REOPEN.most);
    });
  }

  /** The parameters, all of them, with the acknowledgement. */
  private send(): void {
    if (this.socket?.readyState !== OPEN) return;
    this.socket.send(JSON.stringify({ ...this.ask, ack: this.ready }));
  }

  private readyFor(next: number): void {
    if (next <= this.ready) return;
    this.ready = next;
    this.send();
  }

  private async receive(data: unknown): Promise<void> {
    // Text is the server's answer to parameters it couldn't read.
    if (!(data instanceof ArrayBuffer)) return;
    const arrived = this.place.now();
    let read;
    try {
      read = readMessage(data);
    } catch (error) {
      this.stop();
      this.place.post({ kind: "state", state: "refused", reason: (error as Error).message });
      return;
    }
    const { header, jpeg, stats, mask } = read;
    let bitmap: ImageBitmap;
    try {
      bitmap = await this.place.decode(new Blob([jpeg], { type: "image/jpeg" }));
    } catch {
      // A JPEG that won't decode is passed over, as if drawn, so that the
      // stream goes on.
      this.readyFor(header.sequence + 1);
      return;
    }
    const frame: DecodedFrame = { kind: "frame", header, bitmap, arrived, bytes: data.byteLength };
    if (stats) frame.stats = stats;
    if (mask) frame.mask = mask;
    this.place.post(frame, [bitmap, ...(mask ? [mask.bits.buffer] : [])]);
  }

  private stop(): void {
    this.wanted = false;
    clearTimeout(this.timer);
    const socket = this.socket;
    this.socket = undefined;
    socket?.close();
  }
}
