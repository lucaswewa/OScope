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

/**
 * The stage homed, then over the default slide's tissue, as P17's example
 * has it, not empty glass, on the fast clock; and the camera's region.
 */
export async function onTissue(url: string): Promise<void> {
  const on = new Api(url);
  await fast(url, async () => {
    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    await act("/api/v1/stage/move_to", { x: 40_000, y: 12_000, z: 5_000, speed: 25_000 }, { on });
  });
  await aRegion(url);
}
