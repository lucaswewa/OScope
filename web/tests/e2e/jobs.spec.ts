// The jobs tab (W4.1): the queue follows jobs submitted from elsewhere, and
// the rail leads to the running one; reordering, by the keys and by
// dragging, cancelling, and pausing and resuming reach the server; a job's
// end raises its notice, which leads to it; axe; and the tab's baseline.
// On the camera's tiny region, as W3.7's tests: a debug server's frames are
// work.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import type { Job, JobRef } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { overTissueTiny } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const row = (page: Page, id: string) => said(page, "queue").locator(`[data-job="${id}"]`);

/** The camera streaming over the tissue, and a project to capture into: its ID. */
async function ready(url: string): Promise<string> {
  await overTissueTiny(url);
  const on = new Api(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  return (await act("/api/v1/projects/create", { name: "Jobs" }, { on })).id;
}

/** Submits a job of `kind` from outside the page: its ID. */
async function submit(url: string, kind: string, params: unknown): Promise<string> {
  const ref = await act("/api/v1/jobs/submit", { kind, params }, { on: new Api(url) });
  return (ref as JobRef).id;
}

/** The queue, as the server has it. */
async function queue(url: string): Promise<Job[]> {
  const on = new Api(url);
  return (await call((signal) => on.api.GET("/api/v1/jobs/queue", { signal }))) as unknown as Job[];
}
const placeOf = async (url: string, id: string) =>
  (await queue(url)).find((job) => job.id === id)?.position;
const stateOf = async (url: string, id: string) => {
  const on = new Api(url);
  const answer = await call((signal) =>
    on.api.GET("/api/v1/jobs/job/{id}", { params: { path: { id } }, signal }),
  );
  return (answer as unknown as Job).state;
};

/** A burst long enough to hold the hardware while the test works. */
const LONG = (project: string) => ({ project, frames: 5_000, name: "a long burst" });

test("the queue follows jobs submitted elsewhere, and the rail leads to the one running @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  const project = await ready(server.url);
  await page.goto(`${server.url}/#/jobs`);
  await expect(said(page, "queue")).toContainText("Nothing runs");
  const long = await submit(server.url, "capture.burst", LONG(project));
  const first = await submit(server.url, "capture.single", { project, name: "first" });
  const second = await submit(server.url, "capture.single", { project, name: "second" });
  // Running, then the two in line behind it, as they were submitted.
  await expect(row(page, long)).toContainText("Running", { timeout: 30_000 });
  await expect(row(page, first)).toContainText("Number 1 in line");
  await expect(row(page, second)).toContainText("Number 2 in line");
  // The rail's running job leads to it.
  await said(page, "running-job").click();
  await expect(page).toHaveURL(new RegExp(`job=${long}`));
  await expect(said(page, "job")).toContainText("Running");
  // Cancelled from elsewhere: it leaves the queue for the history.
  await act("/api/v1/jobs/cancel", { id: long }, { on: new Api(server.url) });
  await expect(said(page, "history")).toContainText("Cancelled", { timeout: 30_000 });
  await expect(row(page, long)).toHaveCount(0);
});

test("reordering, by the keys and by dragging, and cancelling reach the server @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  const project = await ready(server.url);
  const long = await submit(server.url, "capture.burst", LONG(project));
  const first = await submit(server.url, "capture.single", { project, name: "first" });
  const second = await submit(server.url, "capture.single", { project, name: "second" });
  await page.goto(`${server.url}/#/jobs`);
  await expect(row(page, second)).toContainText("Number 2 in line", { timeout: 30_000 });
  // Alt and the up arrow: the second goes first.
  await row(page, second).focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(() => placeOf(server.url, second)).toBe(0);
  await expect(row(page, second)).toContainText("Number 1 in line");
  // Dragged onto the second's place: the first is first again.
  await row(page, first).dragTo(row(page, second));
  await expect.poll(() => placeOf(server.url, first)).toBe(0);
  // A waiting job is cancelled at once; the running one, asked first. Firefox can take
  // the first press after a drag for the drag's end: the row is pressed until chosen.
  await expect(async () => {
    await row(page, second).click();
    await expect(page).toHaveURL(new RegExp(`job=${second}`), { timeout: 2_000 });
  }).toPass({ timeout: 30_000 });
  await said(page, "job").getByRole("button", { name: "Cancel the job" }).click();
  await expect.poll(() => stateOf(server.url, second)).toBe("cancelled");
  await row(page, long).click();
  await said(page, "job").getByRole("button", { name: "Cancel the job" }).click();
  await page.getByRole("button", { name: "Cancel it" }).click();
  await expect.poll(() => stateOf(server.url, long)).toBe("cancelled");
});

test("pausing and resuming reach the server", async ({ page, server }) => {
  test.setTimeout(150_000);
  await ready(server.url);
  // A batch over the camera's frames, a blur on each: a job that can pause between them.
  const recipe = {
    format: 1,
    name: "A blur",
    inputs: [{ name: "image", type: "image" }],
    nodes: [{ id: "blur", node: "filter.gaussian", params: { sigma: 8 } }],
    edges: [{ from: "input.image", to: "blur.image" }],
    outputs: [{ name: "image", from: "blur.image" }],
  };
  const batch = await submit(server.url, "pipeline.batch", { recipe, frames: 100 });
  await page.goto(`${server.url}/#/jobs?job=${batch}`);
  const detail = said(page, "job");
  await detail.getByRole("button", { name: "Pause" }).click({ timeout: 30_000 });
  await expect.poll(() => stateOf(server.url, batch), { timeout: 30_000 }).toBe("paused");
  await detail.getByRole("button", { name: "Resume" }).click();
  await expect.poll(() => stateOf(server.url, batch), { timeout: 30_000 }).toBe("running");
  await detail.getByRole("button", { name: "Cancel the job" }).click();
  await page.getByRole("button", { name: "Cancel it" }).click();
  await expect.poll(() => stateOf(server.url, batch), { timeout: 30_000 }).toBe("cancelled");
});

test("a job's end raises its notice, which leads to it and its results @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  const project = await ready(server.url);
  await page.goto(`${server.url}/#/control`);
  // A short burst: the rail shows it running, so the shell follows the jobs as it ends.
  const burst = { project, frames: 30, name: "a short burst" };
  const one = await submit(server.url, "capture.burst", burst);
  await expect(said(page, "running-job")).toBeVisible({ timeout: 30_000 });
  const notice = page.locator(".ui-toast").filter({ hasText: "completed" });
  await expect(notice).toBeVisible({ timeout: 60_000 });
  await notice.getByRole("button", { name: "Show it" }).click();
  await expect(page).toHaveURL(new RegExp(`#/jobs\\?job=${one}`));
  await expect(said(page, "job")).toContainText("Completed");
  await expect(said(page, "result").first()).toHaveAttribute("href", /#\/gallery\?project=/);
});

test("the jobs tab has no violation axe can find, in either theme", async ({ page, server }) => {
  test.setTimeout(150_000);
  const project = await ready(server.url);
  const long = await submit(server.url, "capture.burst", LONG(project));
  await submit(server.url, "capture.single", { project, name: "waits" });
  await page.goto(`${server.url}/#/jobs?job=${long}`);
  await expect(said(page, "job")).toContainText("Running", { timeout: 30_000 });
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await act("/api/v1/jobs/cancel", { id: long }, { on: new Api(server.url) });
});

test("the jobs tab looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(150_000);
  const project = await ready(server.url);
  const long = await submit(server.url, "capture.burst", LONG(project));
  const waits = await submit(server.url, "capture.single", { project, name: "waits" });
  await page.goto(`${server.url}/#/jobs?job=${waits}`);
  await expect(row(page, waits)).toContainText("Number 1 in line", { timeout: 30_000 });
  // The rail's indicator says the stage is homed: a moment after it was, as in W3.1's baseline.
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The times and the running job's progress are the moment's: masked.
    await expect(page).toHaveScreenshot(`jobs-${scheme}.png`, {
      mask: [said(page, "job").locator("dd"), row(page, long), said(page, "running-job")],
    });
    await shot(page, `W4.1-jobs-${scheme}`);
  }
  await act("/api/v1/jobs/cancel", { id: long }, { on: new Api(server.url) });
});
