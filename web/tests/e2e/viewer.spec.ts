// Covers: web/src/features/gallery, web/src/ui/tile-viewer
// The deep-zoom viewer (W6.4): an imported slide's tiles arrive for each
// level as it is zoomed into; a stack's slider changes the plane its tiles
// are of; two records side by side move together; a colour map colours a
// one-channel image, and the view is saved as a PNG; axe; the viewer's baseline; and, with OSCOPE_PERF=1,
// the time a full-screen view of a region never seen takes to be sharp.
import { readFile } from "node:fs/promises";

import type { Locator, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { homed } from "./sim.ts";
import { tiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const viewer = (page: Page, record: string) => said(page, `viewer-${record}`);
const tiles = (page: Page, record: string) => viewer(page, record).locator('[data-said="tiles"]');

/** The tiles the page asks for: each one's level and plane. */
function tilesAsked(page: Page): { level: number; plane: number; status?: number }[] {
  const asked: { level: number; plane: number; status?: number }[] = [];
  page.on("response", (response) => {
    const url = new URL(response.url());
    const found = /\/image_files\/(\d+)\/\d+_\d+\.jpg$/.exec(url.pathname);
    if (!found) return;
    const plane = Number(url.searchParams.get("plane") ?? 0);
    asked.push({ level: Number(found[1]), plane, status: response.status() });
  });
  return asked;
}

/** The viewer of `record`, and of `beside` if given, opened. */
async function viewing(page: Page, url: string, project: string, record: string, beside = "") {
  const query = new URLSearchParams({ project, record, view: "1", ...(beside ? { beside } : {}) });
  await page.goto(`${url}/#/gallery?${query.toString()}`);
  await expect(tiles(page, record)).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
}

/** What a viewer's scale bar says, and how long it is. */
async function scaleOf(view: Locator): Promise<string> {
  const scale = view.locator('[data-said="tile-scale"]');
  const width = await scale
    .locator(".line")
    .evaluate((line) => (line as HTMLElement).style.inlineSize);
  return `${await scale.innerText()} ${Math.round(parseFloat(width))}`;
}

/** The colour of the middle of a viewer's canvas. */
function middleOf(view: Locator): Promise<number[]> {
  return view
    .locator("canvas")
    .first()
    .evaluate((canvas) => {
      const c = canvas as HTMLCanvasElement;
      const context = c.getContext("2d")!;
      return [...context.getImageData(c.width >> 1, c.height >> 1, 1, 1).data];
    });
}

test("an image's tiles arrive for each level as it is zoomed into @main", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  const project = await made(server.url, "Zoomed");
  const record = await imported(server.url, project, "slide.tif", tiffOf(2048, 1536));
  const asked = tilesAsked(page);
  await viewing(page, server.url, project, record);
  const zoomIn = viewer(page, record).getByRole("button", { name: "Zoom in" });
  for (let step = 0; step < 8; step++) {
    await zoomIn.click();
    await expect(tiles(page, record)).toHaveAttribute("data-sharp", "true", { timeout: 30_000 });
  }
  // From the coarsest it drew to its pixels, 2048 wide: Deep Zoom's level 11.
  const levels = new Set(asked.map((one) => one.level));
  const coarsest = Math.min(...levels);
  expect(Math.max(...levels)).toBe(11);
  for (let level = coarsest; level <= 11; level++) expect(levels).toContain(level);
  expect(asked.filter((one) => one.status !== 200)).toEqual([]);
});

test("a stack's slider changes the plane its tiles are of", async ({ page, server }) => {
  const project = await made(server.url, "Stacked");
  const record = await imported(server.url, project, "stack.tif", tiffOf(512, 384, 4));
  const asked = tilesAsked(page);
  await viewing(page, server.url, project, record);
  expect(asked.length).toBeGreaterThan(0);
  expect(asked.every((one) => one.plane === 0)).toBe(true);
  const plane = said(page, "look").getByRole("slider", { name: "Plane" });
  await plane.focus();
  await page.keyboard.press("End");
  await expect.poll(() => asked.some((one) => one.plane === 3), { timeout: 30_000 }).toBe(true);
  await expect(tiles(page, record)).toHaveAttribute("data-sharp", "true", { timeout: 30_000 });
});

test("two records side by side move together", async ({ page, server }) => {
  const project = await made(server.url, "Linked");
  const a = await imported(server.url, project, "ramp.tif", tiffOf(1024, 768));
  const grid = (x: number, y: number) => (((x >> 5) + (y >> 5)) % 2) * 50_000;
  const b = await imported(server.url, project, "grid.tif", tiffOf(1024, 768, 1, grid));
  await viewing(page, server.url, project, a, b);
  await expect(tiles(page, b)).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
  const before = await scaleOf(viewer(page, a));
  // Zoomed in on the first: the second follows.
  for (let step = 0; step < 3; step++)
    await viewer(page, a).getByRole("button", { name: "Zoom in" }).click();
  await expect.poll(() => scaleOf(viewer(page, a))).not.toBe(before);
  await expect
    .poll(async () => (await scaleOf(viewer(page, b))) === (await scaleOf(viewer(page, a))))
    .toBe(true);
  // Dragged on the second: the first follows, its pointer on the same place.
  const box = (await tiles(page, b).boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x - box.width / 4, y - box.height / 6, { steps: 4 });
  await page.mouse.up();
  // Once the pan has settled, each view's middle is the same place on the stage.
  const boxA = (await tiles(page, a).boundingBox())!;
  /** The stage's place under the middle of `record`'s view, the pointer moved there afresh. */
  async function middle(
    record: string,
    box: { x: number; y: number; width: number; height: number },
  ) {
    const [mx, my] = [box.x + box.width / 2, box.y + box.height / 2];
    await page.mouse.move(mx + 1, my);
    await page.mouse.move(mx, my);
    return viewer(page, record).locator('[data-said="at"]').innerText();
  }
  await expect
    .poll(async () => (await middle(b, box)) === (await middle(a, boxA)), { timeout: 15_000 })
    .toBe(true);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W6.4-linked-${scheme}`);
  }
});

test("a colour map colours a one-channel image, and the view is saved as a PNG", async ({
  page,
  server,
}) => {
  const project = await made(server.url, "Coloured");
  const record = await imported(server.url, project, "grey.tif", tiffOf(512, 384));
  await viewing(page, server.url, project, record);
  const [r, g, b] = await middleOf(viewer(page, record));
  expect(r === g && g === b).toBe(true);
  await choose(page, "Colour map", "Viridis");
  await expect
    .poll(async () => {
      const [r2, g2, b2] = await middleOf(viewer(page, record));
      return r2 !== g2 || g2 !== b2;
    })
    .toBe(true);
  // A region exported: the view, as it is drawn.
  const saving = page.waitForEvent("download");
  await said(page, "viewer").getByRole("button", { name: "Save the view" }).click();
  const saved = await saving;
  expect(saved.suggestedFilename()).toBe("grey-view.png");
  expect((await readFile(await saved.path())).subarray(1, 4).toString("latin1")).toBe("PNG");
});

test("the viewer has no violation axe can find, in either theme", async ({ page, server }) => {
  const project = await made(server.url, "Checked");
  const a = await imported(server.url, project, "a.tif", tiffOf(512, 384, 3));
  const b = await imported(server.url, project, "b.tif", tiffOf(512, 384));
  await viewing(page, server.url, project, a, b);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the viewer on a fixed slide looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // The stage homed: the rail's indicator says the instrument is OK.
  await homed(server.url);
  const project = await made(server.url, "Fixed");
  const record = await imported(server.url, project, "fixed.tif", tiffOf(1024, 768, 3));
  await viewing(page, server.url, project, record);
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The navigator's own tiles come at their own pace, a level more or less: masked.
    await expect(page).toHaveScreenshot(`viewer-${scheme}.png`, {
      mask: [page.locator(".navigator")],
    });
    await shot(page, `W6.4-viewer-${scheme}`);
  }
});

test("a full-screen view of a region never seen is sharp within 150 ms @perf", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_PERF, "measured on the owner's computer: OSCOPE_PERF=1 there");
  test.setTimeout(600_000);
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  const project = await made(server.url, "Measured");
  const record = await imported(server.url, project, "large.tif", tiffOf(8192, 6144));
  await viewing(page, server.url, project, record);
  await page.getByRole("button", { name: "Full screen" }).click();
  // Near one pixel of the image to one of the screen.
  for (let step = 0; step < 5; step++) {
    await viewer(page, record).getByRole("button", { name: "Zoom in" }).click();
    await expect(tiles(page, record)).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
  }
  // Jumps by the navigator, the image's whole there: a grid of places a fifth
  // of the image apart, more than a screen, each never seen.
  const navigator = (await viewer(page, record).locator(".navigator").boundingBox())!;
  const shown = Math.min(navigator.width / 8192, navigator.height / 6144);
  const [across, down] = [8192 * shown, 6144 * shown];
  const [left, top] = [
    navigator.x + (navigator.width - across) / 2,
    navigator.y + (navigator.height - down) / 2,
  ];
  const times: number[] = [];
  for (let trial = 0; trial < 20; trial++) {
    // Timed in the page: from the click to all the view's tiles come; 10 s at most.
    // Ready before the click.
    await tiles(page, record).evaluate((element) => {
      (globalThis as unknown as { measured: Promise<number> }).measured = new Promise<number>(
        (done) => {
          let start: number | undefined;
          let blurred = false;
          const clicked = () => (start ??= performance.now());
          addEventListener("pointerup", clicked, { capture: true });
          const observer = new MutationObserver(() => {
            const sharp = element.getAttribute("data-sharp") === "true";
            if (!sharp) blurred = true;
            else if (blurred && start !== undefined) end(performance.now() - start);
          });
          const end = (ms: number) => {
            observer.disconnect();
            removeEventListener("pointerup", clicked, { capture: true });
            done(ms);
          };
          observer.observe(element, { attributes: true, attributeFilter: ["data-sharp"] });
          setTimeout(() => end(Number.NaN), 10_000);
        },
      );
    });
    const [column, row] = [trial % 5, Math.floor(trial / 5)];
    await page.mouse.click(left + across * (0.1 + 0.2 * column), top + down * (0.125 + 0.25 * row));
    times.push(
      await page.evaluate(() => (globalThis as unknown as { measured: Promise<number> }).measured),
    );
  }
  console.log(`full-screen views never seen, ms: ${times.map((t) => t.toFixed(0)).join(", ")}`);
  // A view that never blurred, or never sharpened within 10 s, is a NaN: none may be.
  expect(times.filter((ms) => !Number.isFinite(ms))).toEqual([]);
  const sorted = [...times].sort((p, q) => p - q);
  const median = sorted[sorted.length >> 1]!;
  const slowest = sorted.at(-1)!;
  console.log(`median ${median.toFixed(0)} ms, slowest ${slowest.toFixed(0)} ms`);
  expect(median).toBeLessThanOrEqual(150);
});
