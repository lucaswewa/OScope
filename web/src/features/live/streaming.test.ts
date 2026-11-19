import { describe, expect, it } from "vitest";

import { liveState, STALE_AFTER, type Arrivals } from "./streaming";

const at = (more: Partial<Arrivals>): Arrivals => ({
  streaming: true,
  since: 1_000,
  last: undefined,
  gap: undefined,
  now: 2_000,
  ...more,
});

describe("the live image's state", () => {
  it("is off while the camera doesn't stream, or isn't read yet", () => {
    expect(liveState(at({ streaming: false, last: 1_900 }))).toBe("off");
    expect(liveState(at({ streaming: undefined }))).toBe("off");
  });

  it("waits for a frame after the camera starts, a frame from before not counting", () => {
    expect(liveState(at({}))).toBe("waiting");
    expect(liveState(at({ last: 900 }))).toBe("waiting");
    expect(liveState(at({ last: 1_500, gap: 70 }))).toBe("live");
  });

  it("is stale once frames stop coming, after three gaps and never sooner than STALE_AFTER", () => {
    const last = 10_000;
    expect(liveState(at({ last, gap: 70, now: last + STALE_AFTER }))).toBe("live");
    expect(liveState(at({ last, gap: 70, now: last + STALE_AFTER + 1 }))).toBe("stale");
    // A long exposure's frames are slow, not stale.
    expect(liveState(at({ last, gap: 1_500, now: last + 4_000 }))).toBe("live");
    expect(liveState(at({ last, gap: 1_500, now: last + 4_501 }))).toBe("stale");
  });
});
