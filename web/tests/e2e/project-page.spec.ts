// Covers: web/src/features/gallery
// A project's records (W6.2): each kind listed with its details, a
// result's provenance among them; a download's bytes the server's file's;
// a download cut short, then resumed by a range, the server's file whole;
// axe; and the page's baseline. On the simulator, through the camera's
// tiny region over the tissue.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { overTissueTiny } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/**
 * A project of an acquisition, a snake of a few tiles; and, if `all`, the
 * result stitched from it and a report. Its ID and the acquisition's.
 */
async function recorded(
  url: string,
  all: boolean,
): Promise<{ project: string; acquisition: string }> {
  await overTissueTiny(url);
  const on = new Api(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = (await act("/api/v1/projects/create", { name: "Records" }, { on })).id;
  const region = { rect: { origin: { x: 39_920, y: 11_940 }, size: { width: 160, height: 120 } } };
  const scan = await act(
    "/api/v1/scan/run",
    { request: { pattern: "snake", region, positions: [] }, project, name: "a scan" },
    { on },
  );
  const scanned = await finished(scan, { on, timeout: 150_000 });
  const acquisition = scanned.results
    .map((r) => /acquisitions\/([^/]+)$/.exec(r)?.[1])
    .find((id) => id !== undefined)!;
  if (all) {
    const params = { project, acquisition };
    const stitch = await act("/api/v1/jobs/submit", { kind: "stitch.run", params }, { on });
    await finished(stitch, { on, timeout: 120_000 });
    const report = await act(
      "/api/v1/jobs/submit",
      { kind: "report.make", params: { project } },
      { on },
    );
    await finished(report, { on, timeout: 120_000 });
  }
  return { project, acquisition };
}

/** The server's file, as the data route answers it. */
async function serverFile(url: string, project: string, path: string): Promise<Buffer> {
  const answer = await fetch(new URL(`/api/v1/data/${project}/${path}`, url));
  return Buffer.from(await answer.arrayBuffer());
}

test("each kind of record is listed with its details @main", async ({ page, server }) => {
  test.setTimeout(300_000);
  const { project } = await recorded(server.url, true);
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  for (const [kind, count] of [
    ["acquisition", 1],
    ["result", 1],
    ["report", 1],
  ] as const) {
    await expect(said(page, `records-${kind}`).getByRole("listitem")).toHaveCount(count);
  }
  // The acquisition: its camera, its frames, the stage's place.
  await said(page, "records-acquisition").getByRole("button").click();
  const details = said(page, "details");
  await expect(details).toContainText("Exposure");
  await expect(details).toContainText("The stage, at its first frame");
  // The result: how it was made, stitched from the acquisition.
  await said(page, "records-result").getByRole("button").click();
  await expect(said(page, "provenance")).toContainText("a scan");
  await expect(said(page, "provenance")).toContainText("stitch.mosaic");
  // The report: its files.
  await said(page, "records-report").getByRole("button").click();
  await expect(said(page, "files").getByRole("listitem")).not.toHaveCount(0);
});

test("a download's bytes are the server's file's", async ({ page, server }) => {
  test.setTimeout(240_000);
  const { project, acquisition } = await recorded(server.url, false);
  await page.goto(`${server.url}/#/gallery?project=${project}&record=${acquisition}`);
  const saving = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download acquisition.json" }).click();
  const saved = await (await saving).path();
  const path = `acquisitions/${acquisition}/acquisition.json`;
  expect(Buffer.compare(await readFile(saved), await serverFile(server.url, project, path))).toBe(
    0,
  );
});

test("a download cut short resumes by a range, whole", async ({ page, server }) => {
  test.setTimeout(240_000);
  const { project, acquisition } = await recorded(server.url, false);
  const path = `acquisitions/${acquisition}/thumbnail.jpg`;
  const whole = await serverFile(server.url, project, path);
  // The first answer cut at half its length, as a lost connection would; the
  // next, a range of the rest, the server's own.
  const ranges: (string | undefined)[] = [];
  await page.route(`**/api/v1/data/${project}/${path}`, async (route) => {
    ranges.push(route.request().headers().range);
    if (ranges.length > 1) return route.continue();
    await route.fulfill({
      status: 200,
      headers: { "content-type": "image/jpeg", "content-length": String(whole.length) },
      body: whole.subarray(0, whole.length >> 1),
    });
  });
  await page.goto(`${server.url}/#/gallery?project=${project}&record=${acquisition}`);
  await page.getByRole("button", { name: "Download thumbnail.jpg" }).click();
  const row = said(page, "file-thumbnail.jpg");
  await expect(row.getByRole("button", { name: "Resume" })).toBeVisible({ timeout: 15_000 });
  const saving = page.waitForEvent("download");
  await row.getByRole("button", { name: "Resume" }).click();
  const saved = await (await saving).path();
  expect(ranges[1]).toMatch(/^bytes=\d+-$/);
  expect(Buffer.compare(await readFile(saved), whole)).toBe(0);
});

test("a project's page has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(240_000);
  const { project } = await recorded(server.url, false);
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  await expect(said(page, "details")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a project's page looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(300_000);
  const { project } = await recorded(server.url, true);
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  await said(page, "records-result").getByRole("button").click();
  await expect(said(page, "provenance")).toBeVisible();
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // When it was made, and its hashes, differ each time: masked; the picture too.
    await expect(page).toHaveScreenshot(`project-page-${scheme}.png`, {
      mask: [
        page.locator(".records .said"),
        said(page, "details").locator("dd"),
        said(page, "provenance").locator("code"),
        // Its steps, by name and version, which each release changes.
        said(page, "provenance").locator(".said"),
        page.locator("img"),
      ],
    });
    await shot(page, `W6.2-project-${scheme}`);
  }
});
