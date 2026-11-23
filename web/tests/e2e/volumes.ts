// Volumes to look at (W7.8): a bright sphere in a stack of 24 planes, imported
// twice, the second's edge sharper, as a deconvolution's would be, to show
// beside the first.
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

/** A project of the two stacks: its name. */
export async function stacked(url: string): Promise<string> {
  const project = await made(url, "Stacks");
  await imported(url, project, "sphere.tif", tiffOf(...SIZE, SOFT));
  await imported(url, project, "sphere sharper.tif", tiffOf(...SIZE, SHARPER));
  return "Stacks";
}
