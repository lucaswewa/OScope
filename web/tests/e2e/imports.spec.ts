// Covers: web/src/features/gallery, web/src/features/projects
// Files into a project and out of it (W6.3): a TIFF dropped on a project's
// page, and one dropped on its card, each uploaded and imported, its
// acquisition listed; an upload stopped; an export, then its download, the
// server's bytes; a recording waiting, a scan cancelled, ingested into a
// project; axe; and the page's baseline. On the simulator.
import { readFile } from "node:fs/promises";

import type { Locator, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { job } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { homed, overTissueTiny } from "./sim.ts";
import { tiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** Drops `bytes` as the file `name` on `target`, as a drag from the desktop would. */
async function dropOn(target: Locator, name: string, bytes: Buffer): Promise<void> {
  const files = await target.page().evaluateHandle(
    ({ name, bytes }) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)], name, { type: "image/tiff" }));
      return transfer;
    },
    { name, bytes: [...bytes] },
  );
  for (const event of ["dragenter", "dragover", "drop"])
    await target.dispatchEvent(event, { dataTransfer: files });
}

test("a TIFF dropped on a project's page is uploaded and imported @main", async ({
  page,
  server,
}) => {
  const project = await made(server.url, "Dropped");
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  await expect(said(page, "imports")).toContainText("TIFF");
  await dropOn(said(page, "project"), "slide 1.tif", tiffOf(40, 30));
  const row = said(page, "import-slide 1.tif");
  await expect(row).toContainText("slide 1.tif: imported", { timeout: 30_000 });
  await expect(said(page, "records-acquisition").getByRole("listitem")).toHaveCount(1);
  await row.getByRole("button", { name: "Open what slide 1.tif made" }).click();
  await expect(said(page, "record").getByRole("heading", { level: 2 })).toHaveText("slide 1");
  await expect(said(page, "details")).toContainText("40 × 30 × 1, u16");
  // A file no importer reads is refused, never sent.
  await dropOn(said(page, "project"), "notes.txt", Buffer.from("not an image"));
  await expect(said(page, "import-notes.txt")).toContainText("no importer reads this file");
});

test("a TIFF dropped on a project's card is imported into it", async ({ page, server }) => {
  await made(server.url, "Elsewhere");
  await made(server.url, "Carded");
  await page.goto(`${server.url}/#/gallery`);
  const card = said(page, "card").filter({ hasText: "Carded" });
  await expect(card).toBeVisible({ timeout: 30_000 });
  await dropOn(card, "card.tif", tiffOf(24, 16));
  await expect(card).toContainText("1 acquisitions", { timeout: 30_000 });
  await expect(said(page, "card").filter({ hasText: "Elsewhere" })).toContainText("0 acquisitions");
});

test("an upload stopped is said so, and imports nothing", async ({ page, server }) => {
  const project = await made(server.url, "Stopped");
  // The upload never answered, as on a slow link.
  await page.route("**/api/v1/upload/**", () => {});
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  const choosing = page.waitForEvent("filechooser");
  await said(page, "imports").getByRole("button", { name: "Import files…" }).click();
  const tiff = tiffOf(40, 30);
  await (await choosing).setFiles({ name: "slow.tif", mimeType: "image/tiff", buffer: tiff });
  const row = said(page, "import-slow.tif");
  await expect(row).toContainText("uploading");
  await row.getByRole("button", { name: "Stop uploading slow.tif" }).click();
  await expect(row).toContainText("slow.tif: stopped");
  await expect(said(page, "records-acquisition")).toHaveCount(0);
});

test("an export's file downloads, the server's bytes", async ({ page, server }) => {
  const project = await made(server.url, "Exported");
  const acquisition = await imported(server.url, project, "field.tif", tiffOf(40, 30));
  await page.goto(`${server.url}/#/gallery?project=${project}&record=${acquisition}`);
  const pane = said(page, "export");
  await choose(page, "Format", "PNG");
  await pane.getByRole("button", { name: "Export", exact: true }).click();
  const row = said(page, "exported").getByRole("listitem");
  await expect(row).toBeVisible({ timeout: 60_000 });
  const name = (await row.locator(".name").textContent())!;
  expect(name).toMatch(/\.png$/);
  const saving = page.waitForEvent("download");
  await row.getByRole("button", { name: `Download ${name}` }).click();
  const saved = await readFile(await (await saving).path());
  const file = `acquisitions/${acquisition}/exports/${name}`;
  const served = await fetch(new URL(`/api/v1/data/${project}/${file}`, server.url));
  expect(served.status).toBe(200);
  expect(Buffer.compare(saved, Buffer.from(await served.arrayBuffer()))).toBe(0);
  expect(saved.subarray(1, 4).toString("latin1")).toBe("PNG");
});

test("a recording waiting is ingested into the project chosen", async ({ page, server }) => {
  test.setTimeout(240_000);
  // A scan cancelled once it has a tile: its frames wait in the scratch space.
  await overTissueTiny(server.url);
  const on = new Api(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const scans = await made(server.url, "Scans");
  await made(server.url, "Ingested");
  const region = { rect: { origin: { x: 39_850, y: 11_900 }, size: { width: 300, height: 200 } } };
  const request = { pattern: "snake" as const, region, positions: [] };
  const scan = await act(
    "/api/v1/scan/run",
    { request, project: scans, name: "cut short" },
    { on },
  );
  const progress = () => call((signal) => on.api.GET("/api/v1/scan/progress", { signal }));
  await expect
    .poll(async () => (await progress())?.done.length ?? 0, { timeout: 120_000 })
    .toBeGreaterThan(0);
  await act("/api/v1/jobs/cancel", { id: scan.id }, { on });
  await expect
    .poll(async () => (await job(scan.id, { on })).state, { timeout: 60_000 })
    .toBe("cancelled");

  await page.goto(`${server.url}/#/gallery`);
  const waiting = said(page, "waiting");
  await expect(waiting).toContainText("One recording waiting", { timeout: 30_000 });
  await choose(page, "Project", "Ingested");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
    await shot(page, `W6.3-waiting-${scheme}`);
  }
  await waiting.getByRole("button", { name: "Ingest into Ingested" }).click();
  await expect(waiting).toHaveCount(0, { timeout: 120_000 });
  const card = said(page, "card").filter({ hasText: "Ingested" });
  await expect(card).toContainText("1 acquisitions", { timeout: 30_000 });
});

test("a project's imports and exports have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  const project = await made(server.url, "Checked");
  const acquisition = await imported(server.url, project, "field.tif", tiffOf(40, 30));
  await page.goto(`${server.url}/#/gallery?project=${project}&record=${acquisition}`);
  await dropOn(said(page, "project"), "notes.txt", Buffer.from("not an image"));
  await expect(said(page, "import-notes.txt")).toBeVisible();
  await said(page, "export").getByRole("button", { name: "Export", exact: true }).click();
  await expect(said(page, "exported")).toBeVisible({ timeout: 60_000 });
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a project's imports and exports look as their baseline does @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // The stage homed: the rail's indicator says the instrument is OK.
  await homed(server.url);
  const project = await made(server.url, "Imported");
  await page.goto(`${server.url}/#/gallery?project=${project}`);
  await dropOn(said(page, "project"), "slide 1.tif", tiffOf(40, 30));
  await expect(said(page, "import-slide 1.tif")).toContainText("imported", { timeout: 30_000 });
  await dropOn(said(page, "project"), "notes.txt", Buffer.from("not an image"));
  await said(page, "export").getByRole("button", { name: "Export", exact: true }).click();
  await expect(said(page, "exported")).toBeVisible({ timeout: 60_000 });
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  // The import's and the export's toasts, once both have come, dismissed: they name the jobs.
  const toasts = page.getByRole("region", { name: "Notifications (F8)" });
  await expect(toasts.getByRole("listitem")).toHaveCount(2);
  for (let left = 2; left > 0; left--) {
    await toasts.getByRole("button", { name: "Dismiss" }).first().click();
    await expect(toasts.getByRole("listitem")).toHaveCount(left - 1);
  }
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // When it was made, and the export's name, differ each time: masked; the picture too.
    await expect(page).toHaveScreenshot(`imports-${scheme}.png`, {
      mask: [
        page.locator(".records .said"),
        said(page, "details").locator("dd"),
        said(page, "exported").locator(".name"),
        page.locator("img"),
      ],
    });
    await shot(page, `W6.3-imports-${scheme}`);
  }
});
