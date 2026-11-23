// Volumes to look at (W7.8): a bright sphere in a stack of 24 planes, imported
// twice, the second's edge sharper, as a deconvolution's would be, to show
// beside the first; and the simulator's cells, z-scanned and deconvolved, as
// P43's example does, for the notes' screenshots and the try-it.
import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { finished, type JobRef } from "../../src/api/jobs.ts";
import { imported, made } from "./records.ts";
import { tiffOf } from "./tiff.ts";

/** A sphere of radius 14 about the middle of a 48 × 40 × 24 stack, its edge soft over `soft` voxels. */
const sphere = (soft: number) => (x: number, y: number, z: number) => {
  const r = Math.hypot(x - 24, y - 20, (z - 12) * 1.5);
  const inside = Math.min(1, Math.max(0, (14 - r) / soft + 0.5));
  return Math.round(2_000 + 50_000 * inside);
};

/** The stack's size, x, y and z. */
export const SIZE = [48, 40, 24] as const;
/** The two stacks' samples, by voxel. */
export const SOFT = sphere(6);
export const SHARPER = sphere(2);

/**
 * A stack a little larger than the volume's brick holds, 1,100 × 100 × 20
 * voxels against 128³: its brick is coarser than its slices.
 */
export const WIDE = [1_100, 100, 20] as const;
export const GRAIN = (x: number, y: number, z: number) =>
  1_000 + ((x * 37 + y * 101 + z * 1_009) % 30_000);

/** A project of the wide stack, *Wide*: its ID, and the stack's. */
export async function widened(url: string): Promise<{ project: string; acquisition: string }> {
  const project = await made(url, "Wide");
  return { project, acquisition: await imported(url, project, "wide.tif", tiffOf(...WIDE, GRAIN)) };
}

/** A project of the two stacks: its name. */
export async function stacked(url: string): Promise<string> {
  const project = await made(url, "Stacks");
  await imported(url, project, "sphere.tif", tiffOf(...SIZE, SOFT));
  await imported(url, project, "sphere sharper.tif", tiffOf(...SIZE, SHARPER));
  return "Stacks";
}

/** The sensor's middle quarter, in its pixels: binned by three, frames of 912 × 608. */
const MIDDLE = { x: 1_368, y: 912, width: 2_736, height: 1_824 };
const BINNED = { mode: "bin3", format: { kind: "bayer16", pattern: "RGGB", significant_bits: 12 } };
/** The coverslip's middle, where the cells are. */
const CELLS = { x: 47_000, y: 12_500 };

/**
 * The simulated fluorescent cells in widefield at 50×, lit by the blue line
 * at full intensity, binned by three, through `roi`, and z-scanned as `stack`
 * says into a project named `name`: the project's ID, and the raw stack's.
 * Binned, the dim cells take some 25 ms a frame, not seconds.
 */
export async function cellsScanned(
  url: string,
  name: string,
  roi = MIDDLE,
  stack = { below_um: 0.5, above_um: 0.5, step_um: 0.5 },
): Promise<{ project: string; acquisition: string }> {
  const on = new Api(url);
  const job = async (ref: unknown) => finished(ref as JobRef, { on, timeout: 600_000 });
  await act("/api/v1/simulation/load", { name: "fluorescent cells" }, { on });
  await act("/api/v1/objective/fit", { name: "50x" }, { on });
  await act("/api/v1/camera/set_mode", BINNED as never, { on });
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: roi, signal }));
  await call((signal) => on.api.PUT("/api/v1/simulation/confocal", { body: false, signal }));
  await act("/api/v1/stage/home", {}, { on });
  const light = (channel: string, lit: boolean) =>
    act("/api/v1/illumination/set_light", { channel, on: lit, intensity: 1 } as never, { on });
  await light("transmitted", false);
  await light("blue 488", true);
  const z = (await act("/api/v1/simulation/in_focus", CELLS, { on })) as number;
  await act("/api/v1/stage/move_to", { ...CELLS, z }, { on });
  await act("/api/v1/camera/start_streaming", {}, { on });
  await job(await act("/api/v1/exposure/auto", {}, { on }));
  const project = (await act("/api/v1/projects/create", { name }, { on })).id;
  const request = { pattern: "z_scan", at: CELLS, stack };
  const scan = { request, project, name: "cells in widefield" };
  const scanned = await job(await act("/api/v1/scan/run", scan as never, { on }));
  return { project, acquisition: scanned.results[0]!.split("/").pop()! };
}

/**
 * The cells z-scanned over 21 planes 0.5 µm apart through the sensor's middle
 * quarter, and the stack deconvolved by `volume.deconvolve` at the
 * objective's NA: the project's name, *Cells*.
 */
export async function deconvolved(url: string): Promise<string> {
  const on = new Api(url);
  const stack = { below_um: 5, above_um: 5, step_um: 0.5 };
  const { project, acquisition } = await cellsScanned(url, "Cells", MIDDLE, stack);
  const fitted = await call((signal) => on.api.GET("/api/v1/objective/fitted", { signal }));
  const params = { project, acquisition, options: { na: (fitted as { na: number }).na } };
  const submitted = await act("/api/v1/jobs/submit", { kind: "volume.deconvolve", params }, { on });
  await finished(submitted as JobRef, { on, timeout: 600_000 });
  return "Cells";
}
