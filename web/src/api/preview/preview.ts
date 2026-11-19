// The live preview on the page (W1.5, ADR-0047): the worker's frames, the
// newest replacing one not yet drawn; each drawn at the display's next
// refresh, then acknowledged; its numbers over the last second; resting
// while the page is hidden; and P18's statistics and mask, for the app.
import { shallowRef } from "vue";

import { server, session } from "../client.ts";
import type { LiveStats, PreviewHeader, SaturationMask } from "./message.ts";
import type { PreviewAsk } from "./params.ts";
import type { DecodedFrame, FromWorker, StreamState, ToWorker } from "./stream.ts";

/** The preview's numbers: rates and means over the last second. */
export interface PreviewNumbers {
  /** Frames drawn a second. */
  fps: number;
  /**
   * Milliseconds from a frame's exposure ending to its drawing: the
   * server's part, to its message being ready, and the page's, from its
   * arrival. The way between isn't counted: the two clocks differ.
   */
  latency: number;
  serverLatency: number;
  pageLatency: number;
  /** Bytes a second. */
  bytes: number;
  /** Frames drawn since the start. */
  drawn: number;
  /** Frames replaced by a newer one before they were drawn. */
  replaced: number;
  /** The last frame drawn. */
  last: PreviewHeader | undefined;
}

/** How the preview stands: its stream's state, or resting while the page is hidden. */
export type PreviewState = StreamState | "resting" | "stopped";

/** The worker's part the page uses: a browser's Worker, or a test's. */
export interface PreviewWorker {
  postMessage(message: ToWorker): void;
  addEventListener(type: "message", listener: (event: MessageEvent<FromWorker>) => void): void;
  terminate(): void;
}

/** What the client is given: the page's own, or a test's. */
export interface PreviewPlace {
  worker(): PreviewWorker;
  /** Calls `run` at the display's next refresh. */
  frame(run: () => void): void;
  /** Milliseconds since the epoch, as the worker counts them. */
  now(): number;
  hidden(): boolean;
  /** Calls `listener` whenever the page is hidden or shown again; gives the means to stop. */
  onVisibility(listener: () => void): () => void;
}

/** The page's own. */
export const pagePlace: PreviewPlace = {
  worker: () => new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }),
  frame: (run) => requestAnimationFrame(() => run()),
  now: () => performance.timeOrigin + performance.now(),
  hidden: () => document.visibilityState === "hidden",
  onVisibility(listener) {
    document.addEventListener("visibilitychange", listener);
    return () => document.removeEventListener("visibilitychange", listener);
  },
};

/**
 * How long a worker told to close is left to send its socket's close
 * before it is ended, in milliseconds: ended at once, it may not, and the
 * server would keep the session until it noticed.
 */
export const CLOSING = 1_000;

const NONE: PreviewNumbers = {
  fps: 0,
  latency: 0,
  serverLatency: 0,
  pageLatency: 0,
  bytes: 0,
  drawn: 0,
  replaced: 0,
  last: undefined,
};

/** A frame drawn, as the numbers count it. */
interface Drawn {
  at: number;
  server: number;
  page: number;
  bytes: number;
}

/** One live preview: started with its page, and stopped with it. */
export class PreviewClient {
  readonly state = shallowRef<PreviewState>("stopped");
  readonly numbers = shallowRef<PreviewNumbers>(NONE);
  /** The last statistics drawn with a frame. */
  readonly stats = shallowRef<LiveStats>();
  /** The saturation mask drawn with them: none where nothing was saturated. */
  readonly mask = shallowRef<SaturationMask>();
  private worker: PreviewWorker | undefined;
  /** Whether the stream is open: not resting, nor stopped. */
  private live = false;
  private pending: DecodedFrame | undefined;
  private scheduled = false;
  private second: Drawn[] = [];
  private ticker: ReturnType<typeof setInterval> | undefined;
  private unlisten: (() => void) | undefined;

  /**
   * @param paint draws a frame: it takes the frame's bitmap, and closes it.
   * @param ask what the stream is asked for, until `setAsk`.
   */
  constructor(
    private readonly paint: (frame: DecodedFrame) => void,
    private ask: PreviewAsk,
    private readonly place: PreviewPlace = pagePlace,
  ) {}

  start(): void {
    if (this.unlisten) return;
    this.unlisten = this.place.onVisibility(() => this.follow());
    this.follow();
  }

  stop(): void {
    this.unlisten?.();
    this.unlisten = undefined;
    this.rest();
    const worker = this.worker;
    this.worker = undefined;
    if (worker) setTimeout(() => worker.terminate(), CLOSING);
    this.state.value = "stopped";
  }

  /** Asks for this from now on, sent whole at once. */
  setAsk(ask: PreviewAsk): void {
    this.ask = ask;
    this.worker?.postMessage({ kind: "ask", ask });
  }

  /** Open while the page is shown, resting while it is hidden. */
  private follow(): void {
    if (this.place.hidden()) {
      this.rest();
      this.state.value = "resting";
    } else if (!this.live) {
      this.open();
    }
  }

  private open(): void {
    if (!this.worker) {
      const worker = this.place.worker();
      this.worker = worker;
      worker.addEventListener("message", (event) => {
        if (this.worker === worker) this.heard(event.data);
      });
    }
    const url = server.socketAddress("/api/v1/preview/stream");
    this.worker.postMessage({ kind: "open", url, token: session.token, ask: this.ask });
    this.live = true;
    this.ticker = setInterval(() => this.recount(), 1000);
  }

  /** The socket closed by its worker, which stays for the next open. */
  private rest(): void {
    if (!this.live) return;
    this.live = false;
    this.worker?.postMessage({ kind: "close" });
    clearInterval(this.ticker);
    this.pending?.bitmap.close();
    this.pending = undefined;
    this.second = [];
    this.recount();
  }

  private heard(message: FromWorker): void {
    // What was on its way when the stream closed is dropped.
    if (!this.live) {
      if (message.kind === "frame") message.bitmap.close();
      return;
    }
    if (message.kind === "state") {
      this.state.value = message.state;
      return;
    }
    if (this.pending) {
      this.pending.bitmap.close();
      this.numbers.value = { ...this.numbers.value, replaced: this.numbers.value.replaced + 1 };
    }
    this.pending = message;
    if (!this.scheduled) {
      this.scheduled = true;
      this.place.frame(() => this.drawPending());
    }
  }

  private drawPending(): void {
    this.scheduled = false;
    const frame = this.pending;
    this.pending = undefined;
    if (!frame || !this.worker || !this.live) return;
    this.paint(frame);
    this.worker.postMessage({ kind: "drawn", sequence: frame.header.sequence });
    const at = this.place.now();
    const { ready, exposureEnd } = frame.header;
    const server = Number(ready - exposureEnd) / 1e6;
    this.second.push({ at, server, page: at - frame.arrived, bytes: frame.bytes });
    this.recount(frame.header, 1);
    // Statistics come with some frames only; the mask with them, where anything was saturated.
    if (frame.stats) {
      this.stats.value = frame.stats;
      this.mask.value = frame.mask;
    }
  }

  /** The numbers again, over the second before now. */
  private recount(last = this.numbers.value.last, drawn = 0): void {
    const now = this.place.now();
    const second = (this.second = this.second.filter((one) => now - one.at < 1000));
    const mean = (key: "server" | "page") =>
      second.length ? second.reduce((sum, one) => sum + one[key], 0) / second.length : 0;
    const [serverLatency, pageLatency] = [mean("server"), mean("page")];
    this.numbers.value = {
      fps: second.length,
      latency: serverLatency + pageLatency,
      serverLatency,
      pageLatency,
      bytes: second.reduce((sum, one) => sum + one.bytes, 0),
      drawn: this.numbers.value.drawn + drawn,
      replaced: this.numbers.value.replaced,
      last,
    };
  }
}
