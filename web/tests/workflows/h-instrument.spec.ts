// Checkpoint H (W4.3's notes): the instrument run from the app. Jobs queued
// behind a burst, reordered and cancelled; a refusal found in the log, and
// its tail held and followed; a fault injected and cleared on the status in
// full; and the service restarted from Power, the test's own server really
// stopping and starting again.
import type { Browser, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import type { Job } from "../../src/api/jobs.ts";
import { choose, expect, test } from "../e2e/fixtures.ts";
import { TINY } from "../e2e/sim.ts";
import { homeAndMove, makeProject, said, stream } from "./steps.ts";

test.use({ restarts: true });

/** The queue, as the server has it. */
async function queue(url: string): Promise<Job[]> {
  const on = new Api(url);
  return (await call((signal) => on.api.GET("/api/v1/jobs/queue", { signal }))) as unknown as Job[];
}
const row = (page: Page, title: RegExp) =>
  said(page, "queue").getByRole("button").filter({ hasText: title });

/** A second page on Control, as a colleague's, capturing one frame into `project`. */
async function captureElsewhere(browser: Browser, url: string, project: string): Promise<void> {
  const other = await (await browser.newContext()).newPage();
  await other.goto(`${url}/#/control`);
  // Its projects listed, once the page has its server: on two cores, a while.
  await expect(other.getByText("Online")).toBeVisible({ timeout: 60_000 });
  await expect(other.getByRole("combobox", { name: "Project", exact: true })).toBeEnabled({
    timeout: 60_000,
  });
  await choose(other, "Project", project);
  await said(other, "capture-run").click();
  await expect.poll(async () => (await queue(url)).length).toBe(2);
  await other.context().close();
}

test("Checkpoint H: jobs, the log, a fault and a restart, from the app @main", async ({
  browser,
  page,
  server,
}) => {
  test.setTimeout(600_000);
  await page.goto(`${server.url}/#/control`);
  await expect(said(page, "connection")).toHaveText(/Online/);
  await homeAndMove(page, server.url);
  await stream(page, server.url, TINY);
  // An exposure of 100 ms: ten frames a second. Through the tiny region at the default
  // 10 ms, the burst below takes a hundred a second, and on two cores the server then
  // answers a second page too late, which takes it for gone.
  const exposure = said(page, "camera").getByRole("spinbutton", { name: "Exposure" });
  await exposure.fill("100");
  await exposure.press("Enter");
  await makeProject(page, "Checkpoint H");

  // A long burst, then a frame from a second page and an autofocus: the burst runs, and the
  // rest wait in line. A pane's capture runs one at a time: its button waits for it. The
  // demo's 500 frames come in under a minute through the tiny region: 5,000 hold it longer.
  const capture = said(page, "capture");
  await capture.getByRole("spinbutton", { name: "Frames" }).fill("5000");
  await capture.getByRole("spinbutton", { name: "Frames" }).press("Enter");
  await said(page, "capture-run").click();
  await expect(said(page, "running-job")).toBeVisible({ timeout: 30_000 });
  await captureElsewhere(browser, server.url, "Checkpoint H");
  await said(page, "autofocus").getByRole("button", { name: "Focus", exact: true }).click();
  await expect.poll(async () => (await queue(server.url)).length).toBe(3);

  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  const burst = row(page, /capture 5000 frames/);
  const single = row(page, /capture a frame/);
  const focus = row(page, /Autofocus/);
  await expect(burst).toContainText("Running");
  await expect(single).toContainText("Number 1 in line");
  await expect(focus).toContainText("Number 2 in line");
  await expect(said(page, "running-job")).toBeVisible();

  // The autofocus first, by Alt and the up arrow; the waiting capture cancelled; the burst
  // cancelled, asked first; and the autofocus starts.
  await focus.focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(focus).toContainText("Number 1 in line");
  await single.click();
  await said(page, "job").getByRole("button", { name: "Cancel the job" }).click();
  await expect(said(page, "history")).toContainText("Cancelled", { timeout: 30_000 });
  await burst.click();
  await said(page, "job").getByRole("button", { name: "Cancel the job" }).click();
  await page.getByRole("button", { name: "Cancel it" }).click();
  await expect(burst).toHaveCount(0, { timeout: 30_000 });
  await expect(said(page, "history").getByText("Cancelled")).toHaveCount(2);
  await expect(focus).toContainText("Running", { timeout: 30_000 });
  await expect(focus).toHaveCount(0, { timeout: 300_000 });
  await expect(said(page, "history")).toContainText("Completed");

  // A move past a limit, from elsewhere: refused, and found at the log's tail.
  await page.getByRole("link", { name: "Logging", exact: true }).click();
  const records = said(page, "records");
  await choose(page, "At least", "Error");
  await page.getByRole("textbox", { name: "Text" }).fill("out of range");
  // The server takes the request, and the stage refuses it: an error in the log.
  await fetch(`${server.url}/api/v1/stage/move_to`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ x: 999_999, y: 0, z: 0 }),
  });
  const refusal = records.getByText(/out of range/).last();
  await expect(refusal).toBeVisible({ timeout: 30_000 });
  await refusal.click();
  await expect(said(page, "record")).toContainText("out of range");

  // The tail held while scrolled up, counting what came since; then followed again.
  await choose(page, "At least", "Info");
  await page.getByRole("textbox", { name: "Text" }).fill("");
  const on = new Api(server.url);
  for (let n = 1; n <= 40; n++) {
    await act("/api/v1/logs/emit", { level: "info", message: `Checkpoint H ${n}` }, { on });
  }
  await expect(records.getByText("Checkpoint H 40", { exact: true })).toBeVisible();
  await records.evaluate((box) => {
    box.scrollTop = 0;
    box.dispatchEvent(new Event("scroll"));
  });
  for (let n = 41; n <= 43; n++) {
    await act("/api/v1/logs/emit", { level: "info", message: `Checkpoint H ${n}` }, { on });
  }
  await expect(said(page, "since")).toContainText("3 new records");
  await said(page, "since").click();
  await expect(records.getByText("Checkpoint H 43", { exact: true })).toBeInViewport();

  // The status in full: each subsystem, the scratch disk's room; the camera unplugged and back.
  await said(page, "indicator").click();
  await said(page, "status-page").click();
  const camera = page.locator('[data-subsystem="camera"]');
  await expect(page.locator("[data-subsystem]")).not.toHaveCount(0);
  await expect(page.locator('[data-subsystem="disks"]')).toContainText("GB free");
  const unplugged = said(page, "faults").locator('[data-fault="camera_unplugged"]');
  await unplugged.getByRole("button").first().click();
  await expect(camera).toHaveAttribute("data-state", "offline");
  await expect(camera).toContainText("disconnected");
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: Fault");
  await unplugged.getByRole("button").last().click();
  await expect(camera).toHaveAttribute("data-state", "ok", { timeout: 30_000 });
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");

  // The service restarted, its name typed; a dry run's reboot only says what it would run.
  await page.getByRole("link", { name: "Power", exact: true }).click();
  const asked = Date.now();
  const power = said(page, "power");
  const confirm = async (opens: string, word: string, goes: string) => {
    await page.locator(`[data-action="${word}"]`).getByRole("button", { name: opens }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog.getByRole("button", { name: goes, exact: true })).toBeDisabled();
    await dialog.getByRole("textbox").fill(word);
    await dialog.getByRole("button", { name: goes, exact: true }).click();
  };
  await confirm("Restart…", "restart", "Restart");
  await expect(power).toContainText("Restarting");
  await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
  await expect(power).toHaveText("The service restarted, and the app has it again.", {
    timeout: 60_000,
  });
  await page.getByRole("link", { name: "About", exact: true }).click();
  await expect(said(page, "system")).toContainText(/Up for\s*\d+ s/);
  const uptime = await call((signal) => on.api.GET("/api/v1/system/uptime", { signal }));
  expect(uptime * 1000).toBeLessThan(Date.now() - asked);
  await page.getByRole("link", { name: "Power", exact: true }).click();
  await confirm("Reboot…", "reboot", "Reboot");
  await expect(power).toHaveText(/^A dry run: rebooting would have run /);
});
