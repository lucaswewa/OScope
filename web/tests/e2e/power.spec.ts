// Covers: web/src/features/power
// Linux container integration preserves confirmation, hardware exclusion and restart.
import type { Page } from "@playwright/test";
import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, test } from "./fixtures.ts";
import { homed, overTissueTiny } from "./sim.ts";

const said = (page: Page) => page.locator('[data-said="power"]');
const dialog = (page: Page) => page.getByRole("alertdialog");
async function confirm(page: Page): Promise<void> {
  const go = dialog(page).getByRole("button", { name: "Restart", exact: true });
  await expect(go).toBeDisabled();
  await dialog(page).getByRole("textbox").fill("restar");
  await expect(go).toBeDisabled();
  await dialog(page).getByRole("textbox").fill("restart");
  await go.click();
}
async function restart(page: Page): Promise<void> {
  await page.locator('[data-action="restart"]').getByRole("button", { name: "Restart…" }).click();
  await confirm(page);
}

test("only container restart is offered; host power endpoints are absent @main", async ({
  page,
  server,
  request,
}) => {
  await page.goto(server.url + "/#/power");
  await expect(page.locator('[data-action="reboot"], [data-action="shutdown"]')).toHaveCount(0);
  for (const name of ["reboot", "shutdown"]) {
    expect(
      (
        await request.post(server.url + "/api/v1/power/" + name, { data: { confirm: true } })
      ).status(),
    ).toBe(404);
  }
  await restart(page);
  await expect(said(page)).toHaveText("A dry run: the service would have restarted.");
});

test("restart is refused during a hardware job and forcing requires confirmation again", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  const on = new Api(server.url);
  await overTissueTiny(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = (await act("/api/v1/projects/create", { name: "Power" }, { on })).id;
  const burst = await act(
    "/api/v1/jobs/submit",
    { kind: "capture.burst", params: { project, frames: 5_000 } },
    { on },
  );
  await expect
    .poll(() => call((signal) => on.api.GET("/api/v1/jobs/hardware", { signal })), {
      timeout: 60_000,
    })
    .toBe((burst as { id: string }).id);
  await page.goto(server.url + "/#/power");
  await restart(page);
  await expect(said(page)).toContainText("capture.burst is running on the instrument");
  await said(page).getByRole("button", { name: "Force it…" }).click();
  await confirm(page);
  await expect(said(page)).toHaveText("A dry run: the service would have restarted.");
  await act("/api/v1/jobs/cancel", { id: (burst as { id: string }).id }, { on });
});

test.describe("a container that restarts", () => {
  test.use({ restarts: true });
  test("the container exits and its supervisor starts it again; the app reconnects @main", async ({
    page,
    server,
  }) => {
    test.setTimeout(120_000);
    await page.goto(server.url + "/#/power");
    const asked = Date.now();
    await restart(page);
    await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
    await expect(said(page)).toHaveText("The service restarted, and the app has it again.", {
      timeout: 60_000,
    });
    expect(server.failed).toBeUndefined();
    const on = new Api(server.url);
    const uptime = await call((signal) => on.api.GET("/api/v1/system/uptime", { signal }));
    expect(uptime * 1000).toBeLessThan(Date.now() - asked);
  });
});

test("the restart tab and confirmation have no violation axe can find", async ({
  page,
  server,
}) => {
  await page.goto(server.url + "/#/power");
  await restart(page);
  await expect(said(page)).toHaveText("A dry run: the service would have restarted.");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await page.locator('[data-action="restart"]').getByRole("button", { name: "Restart…" }).click();
  await expect(dialog(page)).toBeVisible();
  await expectAccessible(page);
});

test("the container restart tab matches its Linux baseline @visual", async ({ page, server }) => {
  test.skip(
    !process.env.OSCOPE_VISUAL,
    "visual baselines require their qualified rendering environment",
  );
  await homed(server.url);
  await page.goto(server.url + "/#/power");
  await restart(page);
  await expect(said(page)).toHaveText("A dry run: the service would have restarted.");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot("power-container-" + scheme + ".png");
  }
});
