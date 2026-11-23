// Covers: web/src/features/workspace
// 3D volumes and slices (W7.8, ADR-0054): a stack drawn by VTK.js; its three
// slices, each the stack's own plane through it, over its own range; a second
// volume beside it, the two turned together; without WebGL2, the slices
// alone; axe; and a fixed volume's baseline from a fixed camera, cut.
import type { Page } from "@playwright/test";

import { Server as Api } from "../../src/api/client.ts";
import { brick, slice, volumeMeta, type StoredImage } from "../../src/api/media.ts";
import { rangeOf } from "../../src/features/workspace/three-d/transfer.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { cellsScanned, deconvolved, GRAIN, SIZE, SOFT, stacked, WIDE, widened } from "./volumes.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** The Volumes tab, the project `making` makes chosen, and `beside` its first stack if it's given. */
async function opened(
  page: Page,
  url: string,
  beside?: string,
  making: (url: string) => Promise<string> = stacked,
): Promise<void> {
  const project = await making(url);
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Volumes" }).click();
  const volumes = said(page, "volumes");
  await volumes.getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: project, exact: true }).click();
  await expect(volumes.getByRole("combobox", { name: "The stack" })).not.toContainText("Choose");
  if (beside) {
    await volumes.getByRole("combobox", { name: "Beside it" }).click();
    await page.getByRole("option", { name: beside, exact: true }).click();
  }
}

/** A slice's canvas, as its pixels' greys, and the index it's at. */
async function sliceShown(page: Page, axis: "x" | "y" | "z") {
  const canvas = said(page, `slice-${axis}`);
  const index = Number(await canvas.getAttribute("data-index"));
  const { width, height, greys } = await canvas.evaluate((element: HTMLCanvasElement) => {
    const { width, height } = element;
    const data = element.getContext("2d")!.getImageData(0, 0, width, height).data;
    return { width, height, greys: Array.from({ length: width * height }, (_, i) => data[i * 4]!) };
  });
  return { index, width, height, greys };
}

type Pattern = (x: number, y: number, z: number) => number;

/** What the slice at `index` should show: the stack's samples there, grey over their own range. */
function sliceOf(
  axis: "x" | "y" | "z",
  index: number,
  size: readonly [number, number, number] = SIZE,
  written: Pattern = SOFT,
) {
  const [nx, ny, nz] = size;
  const [width, height, at] = {
    z: [nx, ny, (a: number, b: number) => written(a, b, index)],
    y: [nx, nz, (a: number, b: number) => written(a, index, b)],
    x: [ny, nz, (a: number, b: number) => written(index, a, b)],
  }[axis] as [number, number, (a: number, b: number) => number];
  const samples = Array.from({ length: width * height }, (_, i) =>
    at(i % width, Math.floor(i / width)),
  );
  const [least, most] = rangeOf(samples);
  const greys = samples.map((v) => Math.round((255 * (v - least)) / (most - least)));
  return { width, height, greys };
}

test("a stack draws as a volume, and its slices are its own planes, each over its range", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await opened(page, server.url);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "1", { timeout: 30_000 });
  await expect(said(page, "volume").locator("canvas")).toHaveCount(1);
  // Each slice in the middle of its axis, as the sliders start.
  for (const [axis, index] of [
    ["z", 12],
    ["y", 20],
    ["x", 24],
  ] as const) {
    await expect(said(page, `slice-${axis}`)).toHaveAttribute("data-index", String(index));
    const { index: at, ...shown } = await sliceShown(page, axis);
    expect(shown, `the ${axis} slice`).toEqual(sliceOf(axis, at));
  }
  // The z slider to its end, and to its start: the planes there.
  const volumes = said(page, "volumes");
  await volumes.getByRole("slider", { name: /The z slice/ }).press("End");
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", String(SIZE[2] - 1));
  expect(await sliceShown(page, "z")).toEqual({ index: 23, ...sliceOf("z", 23) });
  await volumes.getByRole("slider", { name: /The z slice/ }).press("Home");
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "0");
  expect(await sliceShown(page, "z")).toEqual({ index: 0, ...sliceOf("z", 0) });
  // The brick and the five slices fetched, kept within the budget.
  await expect(volumes.locator('[data-said="kept"]')).toHaveText(
    /Kept: 0\.\d MB, in 6 bricks and slices\./,
  );
  // Its transfer function's middle point dragged up: the line follows.
  const point = said(page, "transfer").locator('[data-point="1"]');
  const line = said(page, "transfer").locator("polyline");
  const before = await line.getAttribute("points");
  const box = (await point.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 20, box.y - 40, { steps: 5 });
  await page.mouse.up();
  await expect(line).not.toHaveAttribute("points", before!);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "1");
});

test("a slice that comes late isn't drawn over the one asked for since", async ({
  page,
  server,
}) => {
  await opened(page, server.url);
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "12");
  // The last plane's slice held until the first's is drawn.
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(/\/slice\?axis=z&index=23&/, async (route) => {
    await held;
    await route.continue();
  });
  const slider = said(page, "volumes").getByRole("slider", { name: /The z slice/ });
  await slider.press("End");
  await slider.press("Home");
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "0");
  release();
  // The late one kept, and the first plane still shown.
  await expect(said(page, "volumes").locator('[data-said="kept"]')).toHaveText(
    /in 6 bricks and slices/,
  );
  expect(await sliceShown(page, "z")).toEqual({ index: 0, ...sliceOf("z", 0) });
});

/** The server's slice of `image` at `index`, at its finest level, grey over its own range. */
async function sliced(on: Api, image: StoredImage, axis: "x" | "y" | "z", index: number) {
  const voxels = await slice(image, { axis, index, format: "f32" }, { on });
  const [width, height] = voxels.size.filter((_, i) => i !== { x: 0, y: 1, z: 2 }[axis]) as [
    number,
    number,
  ];
  const samples = Array.from(
    { length: width * height },
    (_, i) => voxels.samples[i * voxels.channels]!,
  );
  const [least, most] = rangeOf(samples);
  const greys = samples.map((v) => Math.round((255 * (v - least)) / (most - least)));
  return { level: voxels.level, shown: { width, height, greys } };
}

/** Each slice of `image` in the middle of its axis at the finest level, and as the server slices it there. */
async function middlesShown(page: Page, on: Api, image: StoredImage) {
  const finest = (await volumeMeta(image, { on })).levels[0]!;
  const levels = new Set<number>();
  for (const axis of ["z", "y", "x"] as const) {
    const middle = Math.round(0.5 * (finest.size[{ x: 0, y: 1, z: 2 }[axis]]! - 1));
    await expect(said(page, `slice-${axis}`)).toHaveAttribute("data-index", String(middle));
    const { index, ...shown } = await sliceShown(page, axis);
    const server = await sliced(on, image, axis, index);
    expect(shown, `the ${axis} slice`).toEqual(server.shown);
    levels.add(server.level);
  }
  await expect(said(page, "volumes").getByRole("status").last()).toHaveText("");
  return { finest, levels: [...levels] };
}

test("a z-scan's raw stack is sliced at level 1, its mosaic's cells, as the server slices it", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  let made = { project: "", acquisition: "" };
  const making = async (url: string) => ((made = await cellsScanned(url, "Raw")), "Raw");
  await opened(page, server.url, undefined, making);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "1", { timeout: 30_000 });
  const image = { project: made.project, kind: "acquisitions" as const, id: made.acquisition };
  const { finest, levels } = await middlesShown(page, new Api(server.url), image);
  expect([finest.level, ...levels], "a raw mosaic's finest level, and its slices'").toEqual([1, 1]);
});

test("a stack larger than the volume's brick is sliced at full size, in the middle of each axis", async ({
  page,
  server,
}) => {
  let made = { project: "", acquisition: "" };
  const making = async (url: string) => ((made = await widened(url)), "Wide");
  await opened(page, server.url, undefined, making);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "1", { timeout: 30_000 });
  const on = new Api(server.url);
  const image = { project: made.project, kind: "acquisitions" as const, id: made.acquisition };
  expect((await brick(image, { max: 128 ** 3 }, { on })).level, "the brick, coarser").toBe(1);
  const { finest } = await middlesShown(page, on, image);
  expect(finest.size).toEqual([...WIDE]);
  // And they are the planes written.
  for (const axis of ["z", "y", "x"] as const) {
    const { index, ...shown } = await sliceShown(page, axis);
    expect(shown, `the ${axis} slice, as written`).toEqual(sliceOf(axis, index, WIDE, GRAIN));
  }
});

/** A drag across `view`, from its middle. */
async function turned(page: Page, view: string): Promise<void> {
  const box = (await said(page, view).boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 90, y + 30, { steps: 8 });
  await page.mouse.up();
}

test("a second volume beside the first turns as it does, either one turned", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await opened(page, server.url, "sphere sharper");
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "2", { timeout: 30_000 });
  await expect(said(page, "volume-beside").locator("canvas")).toHaveCount(1);
  const camera = (view: string) => said(page, view).getAttribute("data-camera");
  const start = await camera("volume");
  expect(start).toBeTruthy();
  expect(await camera("volume-beside")).toBe(start);
  await turned(page, "volume");
  const once = await camera("volume");
  expect(once).not.toBe(start);
  expect(await camera("volume-beside")).toBe(once);
  await turned(page, "volume-beside");
  const twice = await camera("volume-beside");
  expect(twice).not.toBe(once);
  expect(await camera("volume")).toBe(twice);
  await said(page, "volumes").getByRole("button", { name: "Reset the view" }).click();
  await expect(said(page, "volume")).toHaveAttribute("data-camera", start!);
  await expect(said(page, "volume-beside")).toHaveAttribute("data-camera", start!);
});

test("without WebGL2, the slices are shown alone @main", async ({ page, server }) => {
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      kind: string,
      ...rest: unknown[]
    ) {
      return kind === "webgl2"
        ? null
        : (original as (...args: unknown[]) => unknown).call(this, kind, ...rest);
    } as typeof original;
  });
  await opened(page, server.url);
  await expect(said(page, "volumes").locator('[data-said="no-webgl"]')).toBeVisible();
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "12");
  await expect(said(page, "volume")).toHaveCount(0);
  const { index: at, ...shown } = await sliceShown(page, "y");
  expect(shown).toEqual(sliceOf("y", at));
});

test("the volumes have no violation axe can find, in either theme", async ({ page, server }) => {
  await opened(page, server.url, "sphere sharper");
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "2", { timeout: 30_000 });
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a fixed volume, cut, looks as its baseline does, from a fixed camera @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // VTK.js jitters its rays by Math.random's numbers, drawn again for each scene the page
  // makes as it settles: held at one number, the picture is the same each time.
  await page.addInitScript(() => {
    Math.random = () => 0.5;
  });
  await opened(page, server.url);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "1", { timeout: 30_000 });
  // Cut from x to the middle: the sphere's inside shows.
  const cut = said(page, "volumes").getByRole("slider", { name: "Cut from x" });
  for (let i = 0; i < 10; i++) await cut.press("ArrowRight");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(said(page, "volume")).toHaveScreenshot(`volume-${scheme}.png`);
  }
});

test("the cells' stack and its deconvolution, for the notes' screenshots", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  test.setTimeout(900_000);
  await page.setViewportSize({ width: 1280, height: 1000 });
  await opened(page, server.url, "cells in widefield deconvolved", deconvolved);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "2", { timeout: 30_000 });
  await turned(page, "volume");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.8-volumes-${scheme}`);
  }
});
