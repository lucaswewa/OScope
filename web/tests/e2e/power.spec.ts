// Power (W4.3): reboot and shutdown against the dry run, each confirmed by
// typing its name; a refusal while a hardware job runs, then forced; a
// restart, the test's own server really stopped and started again, with the
// app back on its tab (ADR-0060); axe; and the tab's baseline.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { homed, overTissueTiny } from "./sim.ts";

const said = (page: Page) => page.locator('[data-said="power"]');
const card = (page: Page, action: string) => page.locator(`[data-action="${action}"]`);
const dialog = (page: Page) => page.getByRole("alertdialog");

/** Asks for an action from its card, confirms it by typing `word`, and goes on. */
async function power(page: Page, action: string, opens: string, goes: string): Promise<void> {
  await card(page, action).getByRole("button", { name: opens }).click();
  await confirm(page, action, goes);
}

/** Confirms the open question: its button waits for `word`, typed. */
async function confirm(page: Page, word: string, goes: string): Promise<void> {
  const go = dialog(page).getByRole("button", { name: goes, exact: true });
  await expect(go).toBeDisabled();
  await dialog(page).getByRole("textbox").fill(word.slice(0, -1));
  await expect(go).toBeDisabled();
  await dialog(page).getByRole("textbox").fill(word);
  await go.click();
}

const REBOOT = /^A dry run: rebooting would have run (shutdown\.exe \/r \/t 0|systemctl reboot)\.$/;
const SHUTDOWN =
  /^A dry run: shutting down would have run (shutdown\.exe \/s \/t 0|systemctl poweroff)\.$/;

test("reboot and shutdown only say what they would do, each confirmed by its name @main", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/power`);
  await expect(page.locator('[data-said="dry-run"]')).toContainText(
    "Each action here only says what it would do",
  );
  await power(page, "reboot", "Reboot…", "Reboot");
  await expect(said(page)).toHaveText(REBOOT);
  await power(page, "shutdown", "Shut down…", "Shut down");
  await expect(said(page)).toHaveText(SHUTDOWN);
});

test("an action is refused while a hardware job runs, unless forced", async ({ page, server }) => {
  test.setTimeout(120_000);
  const on = new Api(server.url);
  await overTissueTiny(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = (await act("/api/v1/projects/create", { name: "Power" }, { on })).id;
  const params = { project, frames: 5_000, name: "a long burst" };
  const burst = await act("/api/v1/jobs/submit", { kind: "capture.burst", params }, { on });
  const running = () => call((signal) => on.api.GET("/api/v1/jobs/hardware", { signal }));
  await expect.poll(running, { timeout: 60_000 }).toBe((burst as { id: string }).id);
  await page.goto(`${server.url}/#/power`);
  await power(page, "shutdown", "Shut down…", "Shut down");
  await expect(said(page)).toContainText("capture.burst is running on the instrument");
  // Forced, its name typed again: done, as the dry run does.
  await said(page).getByRole("button", { name: "Force it…" }).click();
  await expect(dialog(page)).toContainText("Force it, past the job running?");
  await confirm(page, "shutdown", "Shut down");
  await expect(said(page)).toHaveText(SHUTDOWN);
  await act("/api/v1/jobs/cancel", { id: (burst as { id: string }).id }, { on });
});

test.describe("a server that restarts", () => {
  test.use({ restarts: true });

  test("a restart stops the server and starts it again, and the app has it back @main", async ({
    page,
    server,
  }) => {
    test.setTimeout(120_000);
    await page.goto(`${server.url}/#/power`);
    // Rebooting and shutting down stay dry runs: only the restart is done.
    await expect(page.locator('[data-said="dry-run"]')).toContainText(
      "Rebooting and shutting down only say what they would do",
    );
    const asked = Date.now();
    await power(page, "restart", "Restart…", "Restart");
    await expect(said(page)).toContainText("Restarting");
    await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
    await expect(said(page)).toHaveText("The service restarted, and the app has it again.", {
      timeout: 60_000,
    });
    expect(server.failed).toBeUndefined();
    // A new server, up for less time than since the restart was asked, on the same address.
    const on = new Api(server.url);
    const uptime = await call((signal) => on.api.GET("/api/v1/system/uptime", { signal }));
    expect(uptime * 1000).toBeLessThan(Date.now() - asked);
    // The tab works as before: a reboot is still only said.
    await power(page, "reboot", "Reboot…", "Reboot");
    await expect(said(page)).toHaveText(REBOOT);
  });
});

test("the power tab has no violation axe can find, in either theme", async ({ page, server }) => {
  await page.goto(`${server.url}/#/power`);
  await power(page, "reboot", "Reboot…", "Reboot");
  await expect(said(page)).toHaveText(REBOOT);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  // And its question, open.
  await card(page, "shutdown").getByRole("button", { name: "Shut down…" }).click();
  await expect(dialog(page)).toBeVisible();
  await expectAccessible(page);
});

test("the power tab looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // The stage homed, and the rail's indicator saying so, as the baseline has it.
  await homed(server.url);
  await page.goto(`${server.url}/#/power`);
  await expect(page.locator('[data-said="indicator"]')).toHaveAccessibleName("The instrument: OK");
  await power(page, "reboot", "Reboot…", "Reboot");
  await expect(said(page)).toHaveText(REBOOT);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The command is the platform's.
    await expect(page).toHaveScreenshot(`power-${scheme}.png`, { mask: [said(page)] });
    await shot(page, `W4.3-power-${scheme}`);
  }
});
