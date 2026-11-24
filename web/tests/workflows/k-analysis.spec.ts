// Checkpoint K (W7.9's notes): the H-DAB slide scanned and stitched, its
// nuclei counted and its DAB measured, shapes measured on it, and the PDF
// made; a recipe drawn from the palette, run live on the camera; and a
// surface and a volume turned. The scan is a snake of four frames through
// the camera's small region at 20x: a debug server renders each frame.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { finished, type JobRef } from "../../src/api/jobs.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { choose, expect, inScheme, shot, test } from "../e2e/fixtures.ts";
import { typeRegion } from "../e2e/scanning.ts";
import { TINY } from "../e2e/sim.ts";
import { surfaced } from "../e2e/surfaces.ts";
import { cellsScanned } from "../e2e/volumes.ts";
import { homeAndMove, makeProject, readout, said, stream } from "./steps.ts";

/** Over the H-DAB tissue, as over the H&E: a snake of four frames there, in mm. */
const AT = { x: 40_000, y: 12_000 };
const SNAKE = [39.94, 11.96, 0.2, 0.13];

/** One of a pane's lists, `field`, set to `option`. */
async function pick(page: Page, pane: string, field: string, option: string | RegExp) {
  await said(page, pane).getByRole("combobox", { name: field }).click();
  await page
    .getByRole("option", { name: option, exact: typeof option === "string" })
    .first()
    .click();
}

test("Checkpoint K: the H-DAB slide scanned and stitched, counted, its DAB measured, annotated and reported @main", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);

  // The H-DAB tissue loaded from Settings; the 20x fitted, over it, focused as by hand.
  await page.goto(`${server.url}/#/settings`);
  await page.getByRole("tab", { name: "Simulation", exact: true }).click();
  const sample = page.locator('[data-sample="H-DAB tissue"]');
  await sample.getByRole("button", { name: "Load it" }).click();
  await expect(said(page, "samples-said")).toHaveText("H-DAB tissue is on the stage.");
  await page.getByRole("link", { name: "Control", exact: true }).click();
  const z = (await act("/api/v1/simulation/in_focus", AT, { on })) as number;
  await homeAndMove(page, server.url, { ...AT, z: Math.round(z * 10) / 10 });
  await choose(page, "Fitted", "20x, NA 0.42");

  // The snake, into a new project, planned; then the camera started, the snake run, and
  // stitched when it ends. The camera is stopped then: a debug server renders each frame on
  // the CPU, which, on two cores, starves its answers.
  await page.getByRole("link", { name: "Scan", exact: true }).click();
  await typeRegion(page, "plan-region", SNAKE);
  await makeProject(page, "Checkpoint K", "run");
  await page.getByRole("link", { name: "Control", exact: true }).click();
  await stream(page, server.url);
  await page.getByRole("link", { name: "Scan", exact: true }).click();
  await choose(page, "Project", "Checkpoint K");
  await said(page, "run").getByRole("button", { name: "Run the scan" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Completed: (\d+) of \1 tiles$/, {
    timeout: 300_000,
  });
  await said(page, "made").getByRole("button", { name: "Stitch it" }).click();
  await expect(said(page, "stitch")).toContainText("Completed", { timeout: 300_000 });
  await page.getByRole("link", { name: "Control", exact: true }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Not streaming");

  // The nuclei counted on the mosaic by the library's count-nuclei; shown, a row leading
  // to its cell.
  await page.getByRole("link", { name: "Workspace", exact: true }).click();
  await page.getByRole("tab", { name: "Analyses" }).click();
  await pick(page, "run", "The project", "Checkpoint K");
  await pick(page, "run", "The image", /stitched/);
  const run = async (recipe: string) => {
    await pick(page, "run", "The recipe", recipe);
    const before = await said(page, "results").locator("li").count();
    await said(page, "run").getByRole("button", { name: "Run" }).click();
    await expect(said(page, "run-said")).toHaveText("Done: its result is listed.", {
      timeout: 300_000,
    });
    await expect(said(page, "results").locator("li")).toHaveCount(before + 1);
  };
  const COUNT = "Nuclei counted";
  const STAINS = "Stains separated: H-DAB, and DAB scored";
  await run(COUNT);
  await said(page, "results")
    .getByRole("button", { name: /^Show it: Nuclei/ })
    .click();
  await expect(said(page, "count")).toHaveText(/^\d+ cells, [\d.]+ per mm²\.$/);
  const cells = Number((await said(page, "count").innerText()).split(" ")[0]);
  expect(cells).toBeGreaterThan(20);
  await expect(said(page, "cells").locator("tbody tr")).toHaveCount(cells);
  await said(page, "cells").getByRole("rowheader", { name: "1", exact: true }).click();
  await expect(said(page, "outlines").locator("polygon.chosen")).toHaveCount(1);

  // DAB measured by stains-hdab: its channel alone, and its score.
  await run(STAINS);
  await said(page, "results")
    .getByRole("button", { name: `Show it: ${STAINS}` })
    .click();
  const channel = said(page, "result").getByRole("combobox", { name: "The channel shown" });
  const dab = page.waitForResponse((answer) => answer.url().includes("channel=1"));
  await channel.click();
  await page.getByRole("option", { name: "DAB", exact: true }).click();
  expect((await dab).status()).toBe(200);
  await expect(said(page, "positive")).toHaveText(/^\d+\.\d\d %$/);
  await expect(said(page, "h-score")).toHaveText(/^\d+\.\d\d$/);

  // Shapes drawn on the mosaic, each measured; their table exported.
  await page.getByRole("tab", { name: "Annotations" }).click();
  await pick(page, "annotate", "The project", "Checkpoint K");
  await pick(page, "annotate", "The image", /stitched/);
  const annotations = page.getByRole("tabpanel", { name: "Annotations" });
  await expect(annotations.locator('[data-said="tile-scale"]')).toBeVisible();
  const tools = page.getByRole("toolbar", { name: "What a press draws" });
  const box = (await annotations.locator('[data-said="tiles"]').boundingBox())!;
  const at = (fx: number, fy: number) => [box.x + box.width * fx, box.y + box.height * fy] as const;
  const drag = async (from: [number, number], to: [number, number]) => {
    await page.mouse.move(...at(...from));
    await page.mouse.down();
    await page.mouse.move(...at(...to), { steps: 6 });
    await page.mouse.up();
  };
  const tool = async (name: string) => {
    await tools.getByRole("button", { name, exact: true }).click();
    await expect(tools.getByRole("button", { name, exact: true })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  };
  await tool("Rectangle");
  await drag([0.2, 0.2], [0.35, 0.4]);
  await tool("Ellipse");
  await drag([0.4, 0.6], [0.55, 0.85]);
  await tool("Polygon");
  for (const [fx, fy] of [
    [0.5, 0.15],
    [0.7, 0.2],
    [0.65, 0.45],
    [0.5, 0.4],
    [0.5, 0.15],
  ] as const) {
    await page.mouse.click(...at(fx, fy));
  }
  const measured = said(page, "measured").locator("tbody tr");
  await expect(measured).toHaveCount(3);
  const saving = page.waitForEvent("download");
  await said(page, "measured").getByRole("link", { name: "Export as CSV" }).click();
  const table = (await readFile(await (await saving).path(), "utf-8")).trim().split("\r\n");
  expect(table).toHaveLength(4);

  // The report on the count and the DAB, ticked, the stitched mosaic not: made, and read as PDF.
  await page.getByRole("tab", { name: "Reports" }).click();
  await pick(page, "reports", "The project", "Checkpoint K");
  const make = said(page, "make");
  await expect(make.getByRole("checkbox", { name: COUNT })).toBeChecked();
  await expect(make.getByRole("checkbox", { name: STAINS })).toBeChecked();
  for (const mosaic of await make.getByRole("checkbox", { name: /stitched/ }).all()) {
    if (await mosaic.isChecked()) await mosaic.click();
  }
  await make.getByRole("button", { name: "Make it" }).click();
  await expect(said(page, "making")).toHaveText("Made: it is shown.", { timeout: 120_000 });
  const shown = page.frameLocator('[data-said="report-html"]');
  await expect(shown.getByRole("heading", { level: 2, name: /Nuclei/ })).toBeVisible();
  await expect(shown.getByRole("heading", { level: 2, name: /Stains separated/ })).toBeVisible();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W10.1-report-${scheme}`);
  }
  await said(page, "read").getByRole("button", { name: "PDF", exact: true }).click();
  const pdf = await page.request.get(
    new URL((await said(page, "report-pdf").getAttribute("data"))!, server.url).href,
  );
  expect((await pdf.body()).subarray(0, 5).toString()).toBe("%PDF-");
});

test("Checkpoint K: a recipe drawn from the palette checks, and runs live on the camera", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);
  // A budget a debug build meets beside other tests, the four nodes timed on a frame of
  // 1920 × 1080: 20 ms is a release build's.
  await call((signal) => on.api.PUT("/api/v1/preview/recipe_budget_ms", { body: 5_000, signal }));
  await page.goto(`${server.url}/#/control`);
  const z = (await act("/api/v1/simulation/in_focus", AT, { on })) as number;
  await homeAndMove(page, server.url, { ...AT, z: Math.round(z * 10) / 10 });
  await stream(page, server.url, TINY);

  // Drawn from the palette, as the demo has it, and joined from the input to the outputs.
  await page.getByRole("link", { name: "Workspace", exact: true }).click();
  const palette = said(page, "palette");
  for (const [text, node] of [
    ["gaussian", "filter.gaussian"],
    ["otsu", "threshold.otsu"],
    ["split", "cells.split"],
    ["measure", "cells.measure"],
  ] as const) {
    await palette.getByRole("textbox", { name: "Find a node" }).fill(text);
    await palette.getByRole("button", { name: `Add ${node}` }).click();
  }
  const port = (kind: "input" | "output", reference: string) =>
    page.locator(`[data-${kind}="${reference}"]`).getByRole("button");
  const join = async (from: string, to: string) => {
    await port("output", from).press("Enter");
    await port("input", to).press("Enter");
  };
  await join("input.image", "gaussian.image");
  await join("gaussian.image", "otsu.image");
  await join("otsu.mask", "split.mask");
  await join("split.labels", "measure.labels");
  await join("input.image", "measure.image");
  // Its outputs: the blurred frame, which live shows as its image, and the nuclei's outlines.
  const output = async (from: string) => {
    await port("output", from).press("Enter");
    await said(page, "node-output")
      .getByRole("button", { name: "A new output: join an output to it" })
      .press("Enter");
  };
  await output("gaussian.image");
  await output("measure.outlines");
  // The blur's sigma has no default: given, the recipe can run. Laid out, each node shows.
  await said(page, "editor").getByRole("button", { name: "Lay it out", exact: true }).click();
  await said(page, "canvas").locator('.vue-flow__node[data-id="gaussian"]').click();
  const sigma = said(page, "pane").getByRole("spinbutton", { name: "Sigma", exact: true });
  await sigma.fill("2");
  await sigma.press("Enter");
  await expect(said(page, "checked")).toHaveText("It can run.");

  // Live: the recipe runs on the camera's frames, its cost said, and each node's on it.
  await said(page, "editor").getByRole("button", { name: "Live", exact: true }).click();
  await expect(said(page, "live")).toContainText("ms budget", { timeout: 60_000 });
  for (const node of ["gaussian", "otsu", "split", "measure"]) {
    await expect(said(page, `node-${node}`).locator('[data-said="cost"]')).toContainText(" ms · ");
  }
  await expect(page.getByRole("img", { name: /^The live image/ })).toBeVisible();
});

/** A drag across `view`, from its middle: the view turned. */
async function turned(page: Page, view: string): Promise<void> {
  const box = (await said(page, view).boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 90, y + 30, { steps: 8 });
  await page.mouse.up();
}

test("Checkpoint K: a surface turns, its profile charted; a volume and its deconvolution turn together, cut and sliced", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);
  // As W7.7's try-it makes it: depth from focus on the reflective chip's dome.
  await surfaced(server.url);
  await page.goto(`${server.url}/#/workspace`);
  await page.getByRole("tab", { name: "Surfaces" }).click();
  await pick(page, "surfaces", "The project", "Chip");
  const surface = said(page, "surface");
  await expect(surface).toHaveAttribute("data-drawn", "true", { timeout: 60_000 });
  // Turned: drawn from elsewhere, its pixels change.
  const drawnNow = async () => (await surface.screenshot()).toString("base64");
  const before = await drawnNow();
  await turned(page, "surface");
  await expect.poll(drawnNow).not.toBe(before);
  const map = (await said(page, "height-map").boundingBox())!;
  await page.mouse.move(map.x + map.width * 0.25, map.y + map.height / 2);
  await page.mouse.down();
  await page.mouse.move(map.x + map.width * 0.75, map.y + map.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect(said(page, "profile-ends")).toHaveText(/-?[\d.]+.*-?[\d.]+/);

  // As W7.8's try-it makes them, smaller: the cells' stack, and its deconvolution.
  const stack = { below_um: 2, above_um: 2, step_um: 0.5 };
  await act("/api/v1/camera/stop_streaming", {}, { on });
  const { project, acquisition } = await cellsScanned(server.url, "Cells", undefined, stack);
  const fitted = (await call((signal) => on.api.GET("/api/v1/objective/fitted", { signal }))) as {
    na: number;
  };
  const params = { project, acquisition, options: { na: fitted.na } };
  const job = await act("/api/v1/jobs/submit", { kind: "volume.deconvolve", params }, { on });
  await finished(job as JobRef, { on, timeout: 300_000 });
  await page.getByRole("tab", { name: "Volumes" }).click();
  await pick(page, "volumes", "The project", "Cells");
  await expect(
    said(page, "volumes").getByRole("combobox", { name: "The stack" }),
  ).not.toContainText("Choose");
  await pick(page, "volumes", "Beside it", /deconvol/i);
  await expect(said(page, "volume")).toHaveAttribute("data-drawn", "2", { timeout: 60_000 });
  // Turned together, either one turned.
  const camera = (view: string) => said(page, view).getAttribute("data-camera");
  const start = await camera("volume");
  await turned(page, "volume");
  await expect.poll(() => camera("volume")).not.toBe(start);
  expect(await camera("volume-beside")).toBe(await camera("volume"));
  // Cut into, and the slices moved.
  const volumes = said(page, "volumes");
  await volumes.getByRole("slider", { name: "Cut from x" }).press("PageUp");
  await volumes.getByRole("slider", { name: /The z slice/ }).press("Home");
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "0");
  await volumes.getByRole("slider", { name: /The z slice/ }).press("End");
  await expect(said(page, "slice-z")).toHaveAttribute("data-index", "8");
});
