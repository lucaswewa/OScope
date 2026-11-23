// Covers: web/src/features/control
// The control tab's actions (W3.7): autofocus to the simulator's true
// focus; a capture making a record, in a project made there, and a burst;
// a live recipe set and cleared, and one that reads a reference, refused
// until the reference is captured; axe; and the panes' baselines.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { download } from "../../src/api/media.ts";
import { projectPage } from "../../src/api/projects.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { colourOf, rgbOf } from "./pixels.ts";
import { offFocus, overTissueTiny } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Control, the camera streaming, its frames drawn. */
async function streaming(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
}

/** The stage's z, as the server has it. */
async function stageZ(url: string): Promise<number> {
  const on = new Api(url);
  const now = await call((signal) => on.api.GET("/api/v1/stage/position", { signal }));
  return now.position.z;
}

// The server does the sweep: one browser is enough.
test("autofocus takes the stage to the simulator's true focus", async ({ page, server }) => {
  test.setTimeout(240_000);
  // 12 µm above the focus, as P34's server test starts. A reach of 20 µm sweeps two
  // thirds of the default's frames: each is the simulator's, rendered at its z, and a
  // debug server's sweep takes several cores.
  const truth = await offFocus(server.url, 12);
  // The notes' window, for their pictures below.
  await page.setViewportSize({ width: 1280, height: 900 });
  await streaming(page, server.url);
  const pane = said(page, "autofocus");
  const reach = pane.getByRole("spinbutton", { name: "Reach either side" });
  await reach.fill("20");
  await reach.press("Enter");
  await pane.getByRole("button", { name: "Focus", exact: true }).click();
  await expect(said(page, "focused")).toContainText("In focus at z", { timeout: 180_000 });
  // Within a quarter of the depth of field, as the server's own test asks.
  const on = new Api(server.url);
  const last = (await call((signal) => on.api.GET("/api/v1/autofocus/last", { signal })))!;
  const error = (await stageZ(server.url)) - truth;
  expect(Math.abs(error)).toBeLessThan(last.depth_of_field_um / 4);
  // The reach typed is the one swept, a coarse step at most past it.
  const swept = last.coarse.map((point) => Math.abs(point.z_um - last.start_um));
  expect(Math.max(...swept)).toBeLessThan(20 + last.depth_of_field_um);
  // The curve, and the frame's measures, read when asked.
  await expect(pane.getByText("The last run's curve")).toBeVisible();
  await pane.getByRole("button", { name: "Measure the frame" }).click();
  await expect(said(page, "measures")).toContainText("Brenner");

  // The notes' pictures: the pane scrolled to the actions, once it scrolls.
  const controls = page.getByRole("region", { name: "The controls" });
  await expect
    .poll(() => controls.evaluate((box) => box.scrollHeight > box.clientHeight))
    .toBe(true);
  // Scrolled at each look: the pane takes the window's height a moment after it, and a
  // scroll before then is undone (W3.7, whose panes made the pane overflow sooner).
  await expect
    .poll(async () => {
      await pane.evaluate((section) => section.scrollIntoView({ block: "start" }));
      return (await pane.boundingBox())!.y;
    })
    .toBeLessThan(100);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W3.7-actions-${scheme}`);
  }
});

test("a capture makes a record in a project made there, and a burst one of its frames @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  await streaming(page, server.url);
  const pane = said(page, "capture");
  await pane.getByRole("textbox", { name: "New project" }).fill("Slides of W3.7");
  await pane.getByRole("button", { name: "Make", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Project", exact: true })).toHaveText(
    "Slides of W3.7",
  );
  await pane.getByRole("textbox", { name: "Name", exact: true }).fill("first");
  await choose(page, "Rendition", "PNG");
  await said(page, "capture-run").click();
  await expect(said(page, "captured")).toContainText("Kept in Slides of W3.7", {
    timeout: 60_000,
  });
  // Its thumbnail shown, and a link to it in the gallery.
  await expect(said(page, "thumbnail")).toHaveJSProperty("complete", true);
  const width = await said(page, "thumbnail").evaluate((img: HTMLImageElement) => img.naturalWidth);
  expect(width).toBeGreaterThan(0);
  const link = said(page, "captured").getByRole("link", { name: "Open it in the gallery" });
  await expect(link).toHaveAttribute("href", /#\/gallery\?project=[\w-]+&record=[\w-]+$/);

  // The server's record: named, a frame, with its rendition.
  const on = new Api(server.url);
  const project = (await projectPage({}, { on })).projects.find((p) => p.name === "Slides of W3.7");
  expect(project?.acquisitions).toBe(1);
  const record = new URL(await link.evaluate((a: HTMLAnchorElement) => a.href)).hash.split(
    "record=",
  )[1]!;
  const rendition = await download(project!.id, `acquisitions/${record}/rendition.png`, { on });
  expect(rendition.type).toBe("image/png");

  // A burst of three, named for its frames.
  await pane.getByRole("textbox", { name: "Name", exact: true }).fill("");
  await pane.getByRole("spinbutton", { name: "Frames" }).fill("3");
  await pane.getByRole("spinbutton", { name: "Frames" }).press("Enter");
  await said(page, "capture-run").click();
  await expect
    .poll(async () => (await projectPage({}, { on })).projects[0]?.acquisitions, {
      timeout: 60_000,
    })
    .toBe(2);
  await expect(said(page, "captured")).toContainText("Kept in Slides of W3.7");
  const held = await (await fetch(`${server.url}/api/v1/projects/project/${project!.id}`)).json();
  const planes = (held.acquisitions as { name: string; image: { planes: number } }[]).map((one) => [
    one.name,
    one.image.planes,
  ]);
  expect(planes).toEqual([
    ["first", 1],
    ["burst of 3", 3],
  ]);
});

test("a live recipe is set and cleared, and one that reads a reference waits for it", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  const on = new Api(server.url);
  await call((signal) => on.api.PUT("/api/v1/camera/exposure", { body: 10, signal }));
  // A debug build's recipes take ten times a release's, and more on two cores: the
  // budget, 20 ms, would refuse them all.
  await call((signal) => on.api.PUT("/api/v1/preview/recipe_budget_ms", { body: 5_000, signal }));
  await streaming(page, server.url);
  // The live recipe, by its name: the answer's type also allows a string.
  const recipe = async () =>
    (await call((signal) => on.api.GET("/api/v1/preview/live_recipe", { signal }))) as {
      name: string;
    } | null;
  const coloured = async () => colourOf(await rgbOf(page, image(page)));
  const before = await coloured();

  // The green channel alone: the image goes grey, and the check's cost is said.
  await choose(page, "Recipe", "The green channel alone");
  await expect.poll(async () => (await recipe())?.name).toBe("The green channel alone");
  await expect.poll(coloured, { timeout: 30_000 }).toBeLessThan(2);
  await expect(said(page, "cost")).toContainText("ms on a");
  // None: the look again, and its colours.
  await choose(page, "Recipe", "None");
  await expect.poll(recipe).toBeNull();
  await expect.poll(coloured, { timeout: 30_000 }).toBeGreaterThan(before / 2);

  // The flat field reads the reference `flat`: refused until it is captured.
  await choose(page, "Recipe", "Flat field");
  await expect(said(page, "missing")).toContainText("flat");
  expect(await recipe()).toBeNull();
  const pane = said(page, "recipe");
  await pane.getByRole("spinbutton", { name: "Frames" }).fill("4");
  await pane.getByRole("button", { name: "Capture the reference" }).click();
  await expect(said(page, "reference")).toContainText("the mean of 4 frames");
  await expect(said(page, "references")).toContainText("flat");
  await choose(page, "Recipe", "Flat field");
  await expect.poll(async () => (await recipe())?.name).toBe("Flat field");
  await expect(said(page, "missing")).toHaveCount(0);
  await choose(page, "Recipe", "None");
  await expect.poll(recipe).toBeNull();
});

test("the tab's actions have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  await streaming(page, server.url);
  await act("/api/v1/projects/create", { name: "Axe" }, { on: new Api(server.url) });
  await page.reload();
  await expect(said(page, "capture")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the tab's actions look as their baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(150_000);
  // Tall enough for the whole pane: the stage's Halt, sticky at its foot, covers a scrolled one.
  await page.setViewportSize({ width: 1280, height: 4000 });
  await overTissueTiny(server.url);
  await streaming(page, server.url);
  await said(page, "capture").getByRole("textbox", { name: "New project" }).fill("Baseline");
  await said(page, "capture").getByRole("button", { name: "Make", exact: true }).click();
  await said(page, "capture-run").click();
  await expect(said(page, "thumbnail")).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // Autofocus before a run: a run's curve is the autofocus test's, and its pictures.
    await expect(said(page, "autofocus")).toHaveScreenshot(`autofocus-${scheme}.png`);
    // The thumbnail is the simulator's: masked.
    await expect(said(page, "capture")).toHaveScreenshot(`capture-${scheme}.png`, {
      mask: [said(page, "thumbnail")],
    });
    await expect(said(page, "recipe")).toHaveScreenshot(`recipe-${scheme}.png`);
  }
});
