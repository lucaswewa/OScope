// Covers: web/src/features
// The shell (W2.1): the rail's nine tabs, each opened, naming the window and
// taking the focus to its heading; the skip link, the rail and the keys from
// the keyboard; the shortcuts' list; an action the server refuses, told in
// its words; the read-only mode between two browsers; axe; and the
// baselines.
import type { Browser, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { Server } from "./server.ts";

const TABS = [
  ["Control", "Control and Live View"],
  ["Scan", "Scan Workflow"],
  ["Workspace", "Workspace"],
  ["Gallery", "Gallery"],
  ["Jobs", "Jobs"],
  ["Settings", "Settings"],
  ["Logging", "Logging"],
  ["About", "About"],
  ["Power", "Power"],
] as const;

const heading = (page: Page) => page.getByRole("heading", { level: 1 });
const rail = (page: Page) => page.getByRole("navigation", { name: "The app's tabs" });
const tab = (page: Page, name: string) => rail(page).getByRole("link", { name, exact: true });
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

test("each tab opens from the rail, names the window and takes the focus to its heading @main", async ({
  page,
  server,
}) => {
  await page.goto(server.url);
  await expect(page).toHaveURL(/#\/control$/);
  await expect(heading(page)).toHaveText("Control and Live View");
  await expect(page).toHaveTitle("Control and Live View · OScope");
  // The component gallery is still at its address, and not in the rail.
  await expect(rail(page).getByRole("link")).toHaveCount(TABS.length);
  for (const [short, title] of TABS.slice(1)) {
    await tab(page, short).click();
    await expect(heading(page)).toHaveText(title);
    await expect(heading(page)).toBeFocused();
    await expect(page).toHaveTitle(`${title} · OScope`);
    await expect(tab(page, short)).toHaveAttribute("aria-current", "page");
  }
  await page.goto(`${server.url}/#/dev/gallery`);
  await expect(heading(page)).toHaveText("The tokens");
});

test("the skip link, the rail and the keys work from the keyboard @main", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/control`);
  await expect(heading(page)).toHaveText("Control and Live View");
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Skip to the content" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("#content")).toBeFocused();
  // The rail is one stop: the arrows, and End, go from tab to tab.
  await tab(page, "Control").focus();
  await page.keyboard.press("ArrowDown");
  await expect(tab(page, "Scan")).toBeFocused();
  await page.keyboard.press("End");
  await expect(tab(page, "Power")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Power");
  // The digits open the tabs in the rail's order, and ? lists the keys.
  await page.keyboard.press("5");
  await expect(heading(page)).toHaveText("Jobs");
  await page.keyboard.press("?");
  const list = page.getByRole("dialog", { name: "Keyboard shortcuts" });
  await expect(list).toContainText("Open Control and Live View");
  await expect(list).toContainText("List these shortcuts");
  await page.keyboard.press("Escape");
  await expect(list).toBeHidden();
});

test("an action the server refuses is told in its words", async ({ page, server }) => {
  await page.goto(`${server.url}/#/control`);
  // Not homed, the stage refuses a move.
  await page.getByRole("button", { name: "Centre", exact: true }).click();
  await expect(page.getByText(/^Refused: /)).toContainText("alarm");
});

const ALICE = ["alice", "alice's password"] as const;
const BOB = ["bob", "bob's password"] as const;

/** A browser of its own, logged in, on the Control tab. */
async function loggedIn(
  browser: Browser,
  url: string,
  [name, password]: readonly [string, string],
) {
  const page = await (await browser.newContext()).newPage();
  await page.goto(`${url}/#/control`);
  await expect(heading(page)).toHaveText("Log in to OScope");
  await page.getByRole("textbox", { name: "Name" }).fill(name);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(heading(page)).toHaveText("Control and Live View");
  return page;
}

test("while another holds the lease, every control is off but what changes nothing", async ({
  browser,
}) => {
  const server = new Server();
  server.addAccount(...ALICE);
  server.addAccount(...BOB);
  await server.start();
  try {
    const alice = await loggedIn(browser, server.url, ALICE);
    await expect(said(alice, "read-only")).toContainText("Nobody is in control");
    await expect(alice.getByRole("button", { name: "Home", exact: true })).toBeDisabled();
    await alice.getByRole("button", { name: "Take control" }).click();
    await expect(said(alice, "read-only")).toBeHidden();
    await expect(alice.getByRole("button", { name: "Home", exact: true })).toBeEnabled();

    const bob = await loggedIn(browser, server.url, BOB);
    await expect(said(bob, "read-only")).toContainText("alice is in control");
    await expect(bob.getByRole("button", { name: "Home", exact: true })).toBeDisabled();
    await bob.getByRole("button", { name: "Ask for control" }).click();
    await expect(said(bob, "read-only")).toContainText("has been asked to hand it over");
    // The theme changes nothing on the instrument: it stays free.
    await tab(bob, "Settings").click();
    await expect(bob.getByRole("combobox", { name: "Theme" })).toBeEnabled();
  } finally {
    await server.dispose();
  }
});

test("the shell has no violation axe can find", async ({ page, server }) => {
  for (const route of ["control", "settings"]) {
    await page.goto(`${server.url}/#/${route}`);
    await expect(heading(page)).toBeVisible();
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectAccessible(page);
    }
  }
});

test("the shell looks as its baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.goto(`${server.url}/#/control`);
  await expect(heading(page)).toHaveText("Control and Live View");
  await expect(page.getByRole("button", { name: "Centre", exact: true })).toBeEnabled();
  // The rail's foot, read (W2.2): the stage waits to be homed.
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: Fault");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`shell-${scheme}.png`, { fullPage: true });
    await shot(page, `W2.1-shell-${scheme}`);
  }
});
