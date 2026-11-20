// The simulator's clock, for the end-to-end tests (W3.1): fifty times as
// fast while the stage homes and moves, which take long at the real one, and
// back to real time before anything streams or is measured.
import { act } from "../../src/api/actions.ts";
import { call, Server as Api } from "../../src/api/client.ts";

/** How many times as fast as real time the clock runs while the stage moves. */
export const FAST = 50;

/** Sets the simulator's clock, at `url`, to run `rate` times as fast as real time. */
export async function clockAt(url: string, rate: number): Promise<void> {
  const on = new Api(url);
  await call((signal) => on.api.PUT("/api/v1/simulation/rate", { body: rate, signal }));
}

/** Runs `moves` on the fast clock, and puts it back to real time after, whatever happened. */
export async function fast<T>(url: string, moves: () => Promise<T>): Promise<T> {
  await clockAt(url, FAST);
  try {
    return await moves();
  } finally {
    await clockAt(url, 1);
  }
}

/** The stage at `url` homed, on the fast clock. */
export async function homed(url: string): Promise<void> {
  await fast(url, () => act("/api/v1/stage/home", {}, { on: new Api(url), timeout: 30_000 }));
}

/**
 * The camera given a 1920 × 1272 region, which keeps its rate. A debug
 * server renders a whole 20 MP sensor on the CPU, and many of them at once,
 * as the end-to-end tests run, starve the browsers: a region is an eighth
 * of that work (W1.5).
 */
export async function aRegion(url: string): Promise<void> {
  const roi = { x: 1776, y: 1176, width: 1920, height: 1272 };
  await call((signal) => new Api(url).api.PUT("/api/v1/camera/roi", { body: roi, signal }));
}

/** Where the default slide's tissue is, in µm, as P17's example has it. */
export const TISSUE = { x: 40_000, y: 12_000 };

/**
 * The stage homed, then over the default slide's tissue, not empty glass,
 * on the fast clock; and the camera's region.
 */
export async function onTissue(url: string): Promise<void> {
  const on = new Api(url);
  await fast(url, async () => {
    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    await act("/api/v1/stage/move_to", { ...TISSUE, z: 5_000, speed: 25_000 }, { on });
  });
  await aRegion(url);
}

/**
 * A small region of the camera: 960 × 648 pixels about the sensor's centre,
 * on its grid of 12. A quarter of `aRegion`'s, for the tests that stream
 * but don't time their frames: a Linux replay's software GL couldn't draw
 * the larger beside W1.5's timed test (W3.1).
 */
export const SMALL = { x: 2256, y: 1500, width: 960, height: 648 };

/** The camera given the small region, which keeps its rate. */
export async function aSmallRegion(url: string): Promise<void> {
  await call((signal) => new Api(url).api.PUT("/api/v1/camera/roi", { body: SMALL, signal }));
}

/** The stage over the tissue, on the fast clock, and the camera given the small region. */
export async function overTissue(url: string): Promise<void> {
  await onTissue(url);
  await aSmallRegion(url);
}

/** Where the slide at `url` is sharpest under the tissue, in µm: the simulator's truth. */
export async function inFocus(url: string): Promise<number> {
  const z = await act("/api/v1/simulation/in_focus", TISSUE, { on: new Api(url) });
  if (typeof z !== "number") throw new Error("the simulator has no focus under the tissue");
  return z;
}

/**
 * A tiny region of the camera: 288 × 216 pixels about the sensor's centre, on
 * its grid of 12, about what P34's server test focuses on. An autofocus sweep
 * renders a frame at each z, and a live recipe runs on each frame: on the
 * small region, W3.7's tests, unpinned in the Windows replay, held the whole
 * computer, and the other tests timed out.
 */
export const TINY = { x: 2592, y: 1716, width: 288, height: 216 };

/** The stage over the tissue, on the fast clock, and the camera given the tiny region. */
export async function overTissueTiny(url: string): Promise<void> {
  await onTissue(url);
  await call((signal) => new Api(url).api.PUT("/api/v1/camera/roi", { body: TINY, signal }));
}

/**
 * The stage over the tissue, `by` µm above where it is sharpest, on the fast
 * clock, and the camera's tiny region: the focus's truth.
 */
export async function offFocus(url: string, by: number): Promise<number> {
  await overTissueTiny(url);
  const truth = await inFocus(url);
  const to = { ...TISSUE, z: truth + by, speed: 25_000 };
  await fast(url, () => act("/api/v1/stage/move_to", to, { on: new Api(url) }));
  return truth;
}
