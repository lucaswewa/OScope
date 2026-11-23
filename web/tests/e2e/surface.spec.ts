// Covers: web/src/features/workspace
// 3D surfaces (W7.7, ADR-0054): a depth-from-focus result drawn by VTK.js,
// and a profile across the simulator's dome rising as its truth does;
// without WebGL2, the height map alone, and its profile; axe; and a fixed
// surface's baseline from a fixed camera.
import type { Page } from "@playwright/test";

import { Server as Api } from "../../src/api/client.ts";
import { brick } from "../../src/api/media.ts";
import { projectContents } from "../../src/api/projects.ts";
import { endsOf, profileOf } from "../../src/features/workspace/three-d/surface.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { DOME, fixedSurface, surfaced } from "./surfaces.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** The Surfaces tab, `project` chosen: its one surface shown. */
async function opened(page: Page, url: string, project = "Chip"): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Surfaces" }).click();
  await said(page, "surfaces").getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: project, exact: true }).click();
  await expect(said(page, "height-map")).toBeVisible({ timeout: 30_000 });
}

/**
 * A line dragged across the height map, from the dome's top to 30 µm right
 * of it: the map is the strip's 66 µm, its middle 15 µm right of the top.
 */
async function profiled(page: Page): Promise<[number, number, number]> {
  const box = (await said(page, "height-map").boundingBox())!;
  const across = (um: number) => box.x + box.width * ((um + 32.83) / 65.66);
  const y = box.y + box.height / 2;
  await page.mouse.move(across(-15), y);
  await page.mouse.down();
  await page.mouse.move(across(15), y, { steps: 6 });
  await page.mouse.up();
  const text = await said(page, "profile-ends").innerText();
  const [from, to, rise] = (text.match(/-?[\d.]+/g) ?? []).map(Number);
  return [from!, to!, rise!];
}

test("a surface draws, and a profile across the dome is its heights', falling as the cap does", async ({
  page,
  server,
}) => {
  test.setTimeout(300_000);
  const { project, truth } = await surfaced(server.url);
  await opened(page, server.url);
  await expect(said(page, "surface")).toHaveAttribute("data-drawn", "true", { timeout: 60_000 });
  await expect(said(page, "surface").locator("canvas")).toHaveCount(1);
  const [from, to, rise] = await profiled(page);
  // The page's profile is the heights' own, as the server gives them, along the same line and band.
  const on = new Api(server.url);
  const results = (
    (await projectContents(project, { on })) as { results: { id: string; name: string }[] }
  ).results;
  const id = results.find((one) => one.name.endsWith(" heights"))!.id;
  const read = await brick(
    { project, kind: "results", id },
    { max: 512 * 512, format: "f32" },
    { on },
  );
  const [width, height] = read.size as [number, number, number];
  const pixelUm = read.voxelUm![0];
  const map = { width, height, pixelUm, heights: Float32Array.from(read.samples) };
  const across = (um: number) => ((um + 32.83) / 65.66) * width * pixelUm;
  const y = (height * pixelUm) / 2;
  const line = profileOf(map, { x: across(-15), y }, { x: across(15), y }, 200, 2);
  const [first, last] = endsOf(line.height);
  expect(Math.abs(from - first!)).toBeLessThan(0.2);
  expect(Math.abs(to - last!)).toBeLessThan(0.2);
  // It falls, as the simulator's cap does over those 30 µm.
  expect((await truth(DOME.x + 30)) - (await truth(DOME.x))).toBeLessThan(0);
  expect(rise).toBeLessThan(0);
  // Its controls: stretched, coloured by height, and reset.
  await said(page, "surfaces").getByRole("slider").press("End");
  await said(page, "surfaces").getByRole("combobox", { name: "Its colour" }).click();
  await page.getByRole("option", { name: "Its height", exact: true }).click();
  await said(page, "surfaces").getByRole("button", { name: "Reset the view" }).click();
  await expect(said(page, "surface")).toHaveAttribute("data-drawn", "true");
});

test("without WebGL2, the height map is shown alone, and its profile drawn @main", async ({
  page,
  server,
}) => {
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
  // The fixed surface: no z-scan, in each browser, beside the other tests.
  await opened(page, server.url, await fixedSurface(server.url));
  await expect(said(page, "surfaces").locator('[data-said="no-webgl"]')).toBeVisible();
  await expect(said(page, "surface")).toHaveCount(0);
  const [from, to] = await profiled(page);
  expect(Number.isFinite(from) && Number.isFinite(to)).toBe(true);
});

test("the surfaces have no violation axe can find, in either theme", async ({ page, server }) => {
  await opened(page, server.url, await fixedSurface(server.url));
  await expect(said(page, "surface")).toHaveAttribute("data-drawn", "true", { timeout: 60_000 });
  await profiled(page);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a fixed surface looks as its baseline does, from a fixed camera @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await opened(page, server.url, await fixedSurface(server.url));
  await expect(said(page, "surface")).toHaveAttribute("data-drawn", "true", { timeout: 60_000 });
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(said(page, "surface")).toHaveScreenshot(`surface-${scheme}.png`);
  }
});

test("the dome's surface and its profile, for the notes' screenshots", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  test.setTimeout(300_000);
  await surfaced(server.url);
  await opened(page, server.url);
  await expect(said(page, "surface")).toHaveAttribute("data-drawn", "true", { timeout: 60_000 });
  await profiled(page);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.7-surface-${scheme}`);
  }
});
