// The live preview on the page (W1.5), with its worker and its page stood
// in for: its worker opened; the newest frame drawn at the next refresh,
// the one it replaced closed; each drawn frame acknowledged; the numbers
// over the last second; resting while hidden; and the statistics published.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { session } from "../client.ts";
import type { LiveStats, SaturationMask } from "./message.ts";
import { CLOSING, PreviewClient, type PreviewPlace, type PreviewWorker } from "./preview.ts";
import type { DecodedFrame, FromWorker, ToWorker } from "./stream.ts";

class FakeWorker implements PreviewWorker {
  readonly posted: ToWorker[] = [];
  terminated = false;
  private listener: ((event: MessageEvent<FromWorker>) => void) | undefined;
  postMessage(message: ToWorker): void {
    this.posted.push(message);
  }
  addEventListener(_type: "message", listener: (event: MessageEvent<FromWorker>) => void): void {
    this.listener = listener;
  }
  terminate(): void {
    this.terminated = true;
  }
  says(message: FromWorker): void {
    this.listener?.({ data: message } as MessageEvent<FromWorker>);
  }
}

/** A page: its workers, its refreshes, its clock and whether it is hidden. */
function pageOf() {
  const workers: FakeWorker[] = [];
  const refreshes: Array<() => void> = [];
  let listener: (() => void) | undefined;
  const page = {
    hidden: false,
    clock: 100_000,
    workers,
    /** The display refreshes: what waited for it runs. */
    refresh: () => refreshes.splice(0).forEach((run) => run()),
    turn(hidden: boolean) {
      page.hidden = hidden;
      listener?.();
    },
  };
  const place: PreviewPlace = {
    worker: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
    frame: (run) => refreshes.push(run),
    now: () => page.clock,
    hidden: () => page.hidden,
    onVisibility(heard) {
      listener = heard;
      return () => (listener = undefined);
    },
  };
  return { page, place };
}

/** Frame `sequence`: its exposure ended 30 ms before its message was ready, which arrived 5 ms ago. */
function frame(sequence: number, clock: number, more: Partial<DecodedFrame> = {}): DecodedFrame {
  return {
    kind: "frame",
    header: {
      quality: 80,
      sequence,
      frame: BigInt(sequence),
      exposureStart: 0n,
      exposureEnd: 20_000_000n,
      ready: 50_000_000n,
      roi: { x: 0, y: 0, width: 5472, height: 3648 },
      width: 1920,
      height: 1280,
    },
    bitmap: { close: vi.fn() } as unknown as ImageBitmap,
    arrived: clock - 5,
    bytes: 100_000,
    ...more,
  };
}

const ASK = { width: 1920, height: 1280, quality: 80 };

describe("the live preview", () => {
  let painted: DecodedFrame[];
  beforeEach(() => {
    painted = [];
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    delete session.token;
  });

  function started() {
    const { page, place } = pageOf();
    const client = new PreviewClient((drawn) => painted.push(drawn), ASK, place);
    client.start();
    return { client, page, worker: page.workers[0]! };
  }

  it("opens its worker on the stream, with the session's token and what it asks", () => {
    session.token = "t1";
    const { worker } = started();
    expect(worker.posted).toEqual([
      {
        kind: "open",
        url: expect.stringMatching(/^ws:.*\/api\/v1\/preview\/stream$/),
        token: "t1",
        ask: ASK,
      },
    ]);
  });

  it("draws the newest frame at the next refresh, closing the one it replaced, then acknowledges it", () => {
    const { client, page, worker } = started();
    const [first, second] = [frame(0, page.clock), frame(1, page.clock)];
    worker.says(first);
    worker.says(second);
    expect(first.bitmap.close).toHaveBeenCalled();
    expect(painted).toEqual([]);
    page.refresh();
    expect(painted).toEqual([second]);
    expect(worker.posted.at(-1)).toEqual({ kind: "drawn", sequence: 1 });
    expect([client.numbers.value.drawn, client.numbers.value.replaced]).toEqual([1, 1]);
    page.refresh();
    expect(painted).toEqual([second]);
  });

  it("counts the last second's frames, and their latency, the server's and the page's", () => {
    const { client, page, worker } = started();
    for (let sequence = 0; sequence < 10; sequence++) {
      worker.says(frame(sequence, page.clock));
      page.refresh();
      page.clock += 100;
    }
    const numbers = client.numbers.value;
    expect([numbers.fps, numbers.serverLatency, numbers.pageLatency, numbers.latency]).toEqual([
      10, 30, 5, 35,
    ]);
    expect([numbers.bytes, numbers.last?.sequence]).toEqual([1_000_000, 9]);
    // Nothing more: the next second counts none.
    page.clock += 1_000;
    vi.advanceTimersByTime(1_000);
    expect([client.numbers.value.fps, client.numbers.value.drawn]).toEqual([0, 10]);
  });

  it("rests while the page is hidden, and opens again when it is shown", () => {
    const { client, page, worker } = started();
    worker.says({ kind: "state", state: "open" });
    expect(client.state.value).toBe("open");
    const waiting = frame(0, page.clock);
    worker.says(waiting);
    page.turn(true);
    // Its worker closes the socket, as a server notices at once, and stays.
    expect([worker.terminated, worker.posted.at(-1), client.state.value]).toEqual([
      false,
      { kind: "close" },
      "resting",
    ]);
    expect(waiting.bitmap.close).toHaveBeenCalled();
    const late = frame(1, page.clock);
    worker.says(late);
    page.refresh();
    expect([painted, (late.bitmap.close as ReturnType<typeof vi.fn>).mock.calls.length]).toEqual([
      [],
      1,
    ]);
    page.turn(false);
    expect(page.workers).toHaveLength(1);
    expect(worker.posted.at(-1)).toMatchObject({ kind: "open", ask: ASK });
  });

  it("sends a new ask whole, at once", () => {
    const { client, worker } = started();
    client.setAsk({ width: 960, height: 640, max_fps: 5 });
    expect(worker.posted.at(-1)).toEqual({
      kind: "ask",
      ask: { width: 960, height: 640, max_fps: 5 },
    });
  });

  it("publishes the statistics drawn, with the mask, which is none where nothing was saturated", () => {
    const { client, page, worker } = started();
    const stats = { focus: 2 } as LiveStats;
    const mask = { width: 1, height: 1, stride: 1, bits: new Uint8Array([0x80]) } as SaturationMask;
    worker.says(frame(0, page.clock, { stats, mask }));
    page.refresh();
    expect([client.stats.value, client.mask.value]).toEqual([stats, mask]);
    worker.says(frame(1, page.clock));
    page.refresh();
    expect([client.stats.value, client.mask.value]).toEqual([stats, mask]);
    const calm = { focus: 3 } as LiveStats;
    worker.says(frame(2, page.clock, { stats: calm }));
    page.refresh();
    expect([client.stats.value, client.mask.value]).toEqual([calm, undefined]);
  });

  it("stops: its socket closed, its worker ended once it could close it, and no frame drawn after", () => {
    const { client, page, worker } = started();
    worker.says(frame(0, page.clock));
    client.stop();
    page.refresh();
    expect([worker.posted.at(-1), worker.terminated, client.state.value, painted]).toEqual([
      { kind: "close" },
      false,
      "stopped",
      [],
    ]);
    vi.advanceTimersByTime(CLOSING);
    expect(worker.terminated).toBe(true);
  });
});
