import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";

import { useServerVersion } from "@/features/home/serverVersion";

describe("the server's version on the first page", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("is looked for, found, lost when the server stops, and found again", async () => {
    const answers = [
      () => Promise.resolve("0.1.1"),
      () => Promise.reject(new Error("no server")),
      () => Promise.resolve("0.1.2"),
    ];
    const ask = vi.fn(() => answers.shift()!());
    const scope = effectScope();
    const state = scope.run(() => useServerVersion(ask, 2000))!;

    expect(state.value).toEqual({ kind: "connecting" });
    await vi.advanceTimersByTimeAsync(0);
    expect(state.value).toEqual({ kind: "connected", version: "0.1.1" });
    await vi.advanceTimersByTimeAsync(2000);
    expect(state.value).toEqual({ kind: "unreachable" });
    await vi.advanceTimersByTimeAsync(2000);
    expect(state.value).toEqual({ kind: "connected", version: "0.1.2" });
    scope.stop();
  });

  it("counts an answer that takes longer than its interval as none", async () => {
    // A server that accepts the request and never answers it.
    const ask = vi.fn(
      (signal: AbortSignal) =>
        new Promise<string>((_, reject) => {
          signal.addEventListener("abort", () => reject(new Error("given up")));
        }),
    );
    const scope = effectScope();
    const state = scope.run(() => useServerVersion(ask, 2000))!;

    await vi.advanceTimersByTimeAsync(1999);
    expect(state.value).toEqual({ kind: "connecting" });
    await vi.advanceTimersByTimeAsync(1);
    expect(state.value).toEqual({ kind: "unreachable" });
    scope.stop();
  });

  it("stops asking, and gives up its request, when its page goes", async () => {
    let given: AbortSignal | undefined;
    const ask = vi.fn((signal: AbortSignal) => {
      given = signal;
      return Promise.resolve("0.1.1");
    });
    const scope = effectScope();
    scope.run(() => useServerVersion(ask, 2000));
    await vi.advanceTimersByTimeAsync(0);
    expect(ask).toHaveBeenCalledTimes(1);

    scope.stop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(given?.aborted).toBe(true);
  });
});
