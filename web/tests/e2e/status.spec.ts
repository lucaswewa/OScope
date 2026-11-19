// The shell's status (W2.2): the instrument's indicator following the stage
// homed, and a fault injected and cleared; the page in place of the tabs
// while the server is gone; another's request for the lease, handed over
// from its notice, between two browsers; the notices kept after they go;
// axe; and the baselines.
import type { Browser, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { Server } from "./server.ts";

const heading = (page: Page) => page.getByRole("heading", { level: 1 });
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const indicator = (page: Page) => said(page, "indicator");
const notices = (page: Page) => page.getByRole("button", { name: /^The notices/ });

/**
 * The simulator's clock fifty times as fast as the real one, for the whole
 * test: homing takes a third of a second, not sixteen. Nothing here streams.
 */
const FAST = `
[simulator]
rate = 50
`;

/** The instrument's state, as the indicator says it to a screen reader. */
const stateIs = (page: Page, state: string) =>
  expect(indicator(page)).toHaveAccessibleName(`The instrument: ${state}`);

test("the indicator follows the stage homed, and a fault injected and cleared @main", async ({
  page,
}) => {
  const server = new Server(FAST);
  await server.start();
  try {
    const on = new Api(server.url);
    await page.goto(`${server.url}/#/control`);
    // The stage waits to be homed: nothing can be acquired.
    await stateIs(page, "Fault");
    await expect(indicator(page)).toHaveAttribute("data-tone", "danger");
    await indicator(page).click();
    const panel = said(page, "status");
    await expect(panel).toContainText("stage: in alarm");
    await expect(panel.getByRole("listitem").filter({ hasText: "The stage" })).toContainText(
      "Fault",
    );
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();

    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    await stateIs(page, "OK");
    await expect(indicator(page)).toHaveAttribute("data-tone", "success");

    await act(
      "/api/v1/simulation/inject_fault",
      { fault: "camera_unplugged", active: true },
      { on },
    );
    await stateIs(page, "Fault");
    await indicator(page).click();
    await expect(panel.getByRole("listitem").filter({ hasText: "The camera" })).toContainText(
      "Offline: disconnected",
    );
    await page.keyboard.press("Escape");

    await act(
      "/api/v1/simulation/inject_fault",
      { fault: "camera_unplugged", active: false },
      { on },
    );
    await stateIs(page, "OK");
  } finally {
    await server.dispose();
  }
});

test("the tabs give way to a page while the server is gone, and come back with it @main", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/settings`);
  await expect(heading(page)).toHaveText("Settings");
  await expect(said(page, "connection")).toHaveText(/Online/);

  await server.stop();
  await expect(heading(page)).toHaveText("The server can't be reached");
  await expect(said(page, "connection")).toHaveText(/Lost/);
  await expect(said(page, "no-server")).toContainText("Connecting again");
  // Without the server, the instrument's state isn't known.
  await stateIs(page, "Unknown");

  await server.start();
  await expect(heading(page)).toHaveText("Settings");
  await expect(said(page, "connection")).toHaveText(/Online/);
  await expect(said(page, "no-server")).toHaveCount(0);
  await stateIs(page, "Fault");
});

// In Chromium: WebKit keeps a module's failure, even across a reload, so a
// page that couldn't load stays so there (W2.2's notes).
test("a tab pressed while the server is gone is loaded once it is back", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/settings`);
  await expect(heading(page)).toHaveText("Settings");
  await server.stop();
  await expect(heading(page)).toHaveText("The server can't be reached");
  // The rail still goes from tab to tab, behind the page; About's code can't load.
  await page.getByRole("link", { name: "About", exact: true }).click();
  await expect(heading(page)).toHaveText("The server can't be reached");
  await server.start();
  await expect(heading(page)).toHaveText("About");
});

const ALICE = ["alice", "alice's password"] as const;
const BOB = ["bob", "bob's password"] as const;

/** A browser of its own, logged in, on the Control tab. */
async function loggedIn(
  browser: Browser,
  url: string,
  [name, password]: readonly [string, string],
): Promise<Page> {
  const page = await (await browser.newContext()).newPage();
  await page.goto(`${url}/#/control`);
  await expect(heading(page)).toHaveText("Log in to OScope");
  await page.getByRole("textbox", { name: "Name" }).fill(name);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(heading(page)).toHaveText("Control and Live View");
  return page;
}

test("another's request for the lease is a notice that hands it over", async ({ browser }) => {
  test.setTimeout(60_000);
  const server = new Server();
  server.addAccount(...ALICE);
  server.addAccount(...BOB);
  await server.start();
  try {
    const alice = await loggedIn(browser, server.url, ALICE);
    await expect(said(alice, "holder")).toContainText("Nobody");
    await alice.getByRole("button", { name: "Take control" }).click();
    await expect(said(alice, "holder")).toContainText("You");

    const bob = await loggedIn(browser, server.url, BOB);
    await expect(said(bob, "holder")).toContainText("alice");
    await bob.getByRole("button", { name: "Ask for control" }).click();

    const asked = alice.getByRole("region", { name: /^Notifications/ }).getByRole("listitem");
    await expect(asked).toContainText("bob asks for control of the instrument.");
    await asked.getByRole("button", { name: "Hand over" }).click();
    await expect(said(alice, "holder")).toContainText("bob");
    await expect(said(alice, "read-only")).toContainText("bob is in control");
    await expect(said(bob, "holder")).toContainText("You");
    await expect(said(bob, "read-only")).toBeHidden();
    // The notice went with the hand-over; its history keeps it.
    await expect(asked).toHaveCount(0);
    await notices(alice).click();
    await expect(said(alice, "notices")).toContainText("bob asks for control of the instrument.");
  } finally {
    await server.dispose();
  }
});

test("the notices are kept after they go, and those not yet read are counted", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/control`);
  await expect(notices(page)).toHaveAccessibleName("The notices");
  // Not homed, the stage refuses a move: what failed stays until it is sent away.
  await page.getByRole("button", { name: "Centre", exact: true }).click();
  const refused = page.getByRole("region", { name: /^Notifications/ }).getByRole("listitem");
  await expect(refused).toContainText("Refused:");
  await refused.getByRole("button", { name: "Dismiss" }).click();
  await expect(refused).toHaveCount(0);
  await expect(notices(page)).toHaveAccessibleName("The notices: 1 new");

  await notices(page).click();
  const history = said(page, "notices");
  await expect(history.getByRole("listitem")).toHaveCount(1);
  await expect(history).toContainText("Refused:");
  await page.keyboard.press("Escape");
  await expect(notices(page)).toHaveAccessibleName("The notices");
});

test("the notices' history lists those told, the newest first, each with when", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/dev/gallery/overlays`);
  for (const name of ["Tell", "Succeed", "Warn"]) {
    await page.getByRole("button", { name, exact: true }).click();
  }
  await page.getByRole("button", { name: "Those told" }).click();
  const told = said(page, "notices").getByRole("listitem");
  await expect(told).toHaveText([/\(3\)/, /\(2\)/, /\(1\)/]);
  await expect(told.first().locator("time")).toHaveAttribute("datetime", /^\d{4}-\d\d-\d\dT/);
});

test("the status, the notices and the page without a server have no violation axe can find", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/control`);
  await stateIs(page, "Fault");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await indicator(page).click();
    await expectAccessible(page, "[data-said='status']");
    await page.keyboard.press("Escape");
    await notices(page).click();
    await expectAccessible(page, "[data-said='notices']");
    await page.keyboard.press("Escape");
  }
  await server.stop();
  await expect(heading(page)).toHaveText("The server can't be reached");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

/**
 * A status of words that stay the same: the server's own tells the GPU's
 * temperature and the disk's free space, which change from run to run. One
 * subsystem in each state.
 */
const STEADY = {
  state: "fault",
  message: "stage: in alarm: homing required",
  subsystems: [
    ["camera", "ok", "idle"],
    ["stage", "fault", "in alarm: homing required"],
    ["lights", "ok", "1 of 6 on"],
    ["gpu", "offline", "no GPU"],
    ["disks", "degraded", "low: the scratch disk: 30 GB free, filling at 0 MB/s"],
    ["jobs", "ok", "0 running, 0 queued"],
    ["extensions", "ok", "0 loaded"],
  ].map(([subsystem, state, message]) => ({ subsystem, state, message, details: null })),
};

test("the status and the page without a server look as their baselines do @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.route("**/api/v1/status/instrument", (route) => route.fulfill({ json: STEADY }));
  await page.goto(`${server.url}/#/control`);
  await stateIs(page, "Fault");
  await expect(page.getByRole("button", { name: "Centre", exact: true })).toBeEnabled();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await indicator(page).click();
    await expect(said(page, "status")).toContainText("30 GB free");
    await page.mouse.move(600, 100);
    await expect(page).toHaveScreenshot(`status-${scheme}.png`, { fullPage: true });
    await shot(page, `W2.2-status-${scheme}`);
    // Closed by the pointer, as it was opened: Escape would ring the indicator next time.
    await page.mouse.click(900, 600);
    await expect(said(page, "status")).toBeHidden();
  }
  await server.stop();
  await expect(heading(page)).toHaveText("The server can't be reached");
  await stateIs(page, "Unknown");
  // The focus, back on the indicator, would ring it.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The spinner stands still with less motion asked for: the page is the same each time.
    await expect(page).toHaveScreenshot(`no-server-${scheme}.png`, { fullPage: true });
    await shot(page, `W2.2-no-server-${scheme}`);
  }
});
