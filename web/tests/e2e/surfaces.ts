// A surface to measure (W7.7): the simulator's reflective chip, z-scanned at
// 50× in reflected light over a die's dome, and depth from focus run on the
// stack, as P42's example does through the API. Its polished terraces have
// nothing to focus on, and depth from focus finds no height there; the
// dome's curve it measures, as P42 found.
import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { finished, type JobRef } from "../../src/api/jobs.ts";
import { analyse } from "../../src/api/workspace.ts";
import { imported, made } from "./records.ts";
import { tiffOf } from "./tiff.ts";

/**
 * The coverslip's middle is a die's centre, (47 000, 12 500) µm; the die's
 * dome is 50 µm right of it and 50 µm up, a cap 80 µm across and 3 µm high.
 */
export const DOME = { x: 47_050, y: 12_450 };
/** The frame's part read: all of its width, some 66 µm at 50×, and a strip of 156 rows. */
const STRIP = { x: 0, y: 378, width: 1368, height: 156 };
/** Its middle, 15 µm right of the dome's top: the strip from 18 µm left of the top to 48 µm right. */
export const AT = { x: DOME.x + 15, y: DOME.y };

/** The project, depth from focus's results in it, and the simulator's height at a place, in µm. */
export async function surfaced(
  url: string,
): Promise<{ project: string; truth: (x: number) => Promise<number> }> {
  const on = new Api(url);
  const job = async (ref: unknown) => finished(ref as JobRef, { on, timeout: 240_000 });
  await act("/api/v1/simulation/load", { name: "reflective surface" }, { on });
  await act("/api/v1/objective/fit", { name: "50x" }, { on });
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: STRIP, signal }));
  await act("/api/v1/stage/home", {}, { on });
  // Each light at the server's own intensity.
  const light = (channel: string, lit: boolean) =>
    act("/api/v1/illumination/set_light", { channel, on: lit } as never, { on });
  await light("transmitted", false);
  await light("reflected", true);
  const truth = async (x: number) =>
    (await act("/api/v1/simulation/in_focus", { x, y: AT.y }, { on })) as number;
  const top = await truth(DOME.x);
  await act("/api/v1/stage/move_to", { ...AT, z: top }, { on });
  await act("/api/v1/camera/start_streaming", {}, { on });
  await job(await act("/api/v1/exposure/auto", {}, { on }));
  const project = (await act("/api/v1/projects/create", { name: "Chip" }, { on })).id;
  // From 3 µm under the top to 1 µm over it: the cap's fall over the strip is under 3 µm.
  const stack = { below_um: 3, above_um: 1, step_um: 0.5 };
  const request = { pattern: "z_scan", at: AT, stack };
  const scan = { request, project, name: "a dome" };
  const scanned = await job(await act("/api/v1/scan/run", scan as never, { on }));
  const acquisition = scanned.results[0]!.split("/").pop()!;
  const params = { project, acquisition };
  await job(await act("/api/v1/jobs/submit", { kind: "volume.depth_from_focus", params }, { on }));
  return { project, truth };
}

/**
 * A fixed surface, for a baseline: a smooth dome of 64 × 48 heights, as an
 * image, imported, and run through a gentle blur into a result named as
 * depth from focus names its heights. The simulator's heights differ from
 * run to run; these don't.
 */
export async function fixedSurface(url: string): Promise<string> {
  const on = new Api(url);
  const project = await made(url, "Fixed");
  const dome = (x: number, y: number) =>
    Math.round(60_000 * Math.max(0, 1 - ((x - 32) ** 2 + (y - 24) ** 2) / 900));
  const acquisition = await imported(url, project, "dome.tif", tiffOf(64, 48, 1, dome));
  const recipe = {
    format: 1,
    name: "the same",
    inputs: [{ name: "image", type: "image" as const }],
    nodes: [{ id: "smooth", node: "filter.gaussian", params: { sigma: 1 } }],
    edges: [{ from: "input.image", to: "smooth.image" }],
    outputs: [{ name: "heights", from: "smooth.image" }],
  };
  const input = { kind: "acquisition" as const, id: acquisition };
  const ran = await analyse(project, { recipe, input, name: "fixed heights" }, { on });
  await finished(ran, { on, timeout: 60_000 });
  return "Fixed";
}
