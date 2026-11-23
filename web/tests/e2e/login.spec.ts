// Covers: web/src/features/login, web/src/features/dev
// The login and the control lease (W1.3), with two browsers on a server
// that has accounts: a wrong password, a login, the lease taken, asked for
// and handed over, and a session that ends.
import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { Server } from "./server.ts";

const ALICE = ["alice", "alice's password"] as const;
const BOB = ["bob", "bob's password"] as const;
const PAGE = "/#/dev/gallery/connection";

/** A server of the test's own, with two accounts: the caller disposes of it. */
async function withAccounts(): Promise<Server> {
  const server = new Server();
  server.addAccount(...ALICE);
  server.addAccount(...BOB);
  await server.start();
  return server;
}

/** Logs in on the login the page is sent to, and waits for the page it asked for. */
async function logIn(page: Page, [name, password]: readonly [string, string]): Promise<void> {
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Log in to OScope");
  await page.getByRole("textbox", { name: "Name" }).fill(name);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("The connection");
}

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

test("a wrong password is refused, and the right one lets the user in @main", async ({ page }) => {
  const server = await withAccounts();
  try {
    await page.goto(server.url + PAGE);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Log in to OScope");
    await page.getByRole("textbox", { name: "Name" }).fill("alice");
    await page.getByLabel("Password").fill("not alice's password");
    await page.getByRole("button", { name: "Log in" }).click();
    await expect(page.getByLabel("Password")).toHaveAccessibleDescription(
      "That name and password don't match an account.",
    );
    await page.getByLabel("Password").fill(ALICE[1]);
    await page.getByRole("button", { name: "Log in" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("The connection");
    await expect(said(page, "session")).toContainText("logged in");
    await expect(said(page, "session")).toContainText("alice");
    await expect(said(page, "connection")).toContainText("connected");
  } finally {
    await server.dispose();
  }
});

test("the lease is taken, asked for and handed over, between two browsers", async ({ browser }) => {
  test.setTimeout(60_000);
  const server = await withAccounts();
  const contexts = [await browser.newContext(), await browser.newContext()];
  try {
    const [alice, bob] = await Promise.all(contexts.map((context) => context.newPage()));
    await alice!.goto(server.url + PAGE);
    await logIn(alice!, ALICE);
    await bob!.goto(server.url + PAGE);
    await logIn(bob!, BOB);
    const control = (page: Page) =>
      page.getByRole("switch", { name: "A control of the instrument" });
    // Alice takes it: her controls may change things, Bob's are read-only.
    await alice!.getByRole("button", { name: "Take control" }).click();
    await expect(said(alice!, "control")).toHaveText("This app may change the instrument.");
    await expect(said(bob!, "control")).toHaveText("Read-only: another holds the control lease.");
    await expect(control(bob!)).toHaveAttribute("aria-readonly", "true");
    await expect(said(bob!, "lease")).toContainText("alice");
    // Bob asks; Alice sees it, and hands it over.
    await bob!.getByRole("button", { name: "Ask for control" }).click();
    await expect(said(alice!, "lease")).toContainText("bob");
    await alice!.getByRole("combobox", { name: "Hand it over to" }).click();
    await alice!.getByRole("option", { name: "bob" }).click();
    await alice!.getByRole("button", { name: "Hand it over" }).click();
    await expect(said(bob!, "control")).toHaveText("This app may change the instrument.");
    await expect(said(alice!, "control")).toHaveText("Read-only: another holds the control lease.");
    await expect(control(alice!)).toHaveAttribute("aria-readonly", "true");
  } finally {
    for (const context of contexts) await context.close();
    await server.dispose();
  }
});

test("a session that ends returns to the login, then to where the user was", async ({ page }) => {
  const server = await withAccounts();
  try {
    await page.goto(server.url + PAGE);
    await logIn(page, ALICE);
    // Its session ends on the server: logged out from elsewhere.
    const token = await page.evaluate(() => sessionStorage.getItem("oscope.session"));
    const out = await fetch(`${server.url}/api/v1/auth/logout`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
    });
    expect(out.status).toBe(204);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Log in to OScope");
    await expect(page.getByText("Your session ended.")).toBeVisible();
    await logIn(page, ALICE);
    await expect(page).toHaveURL(new RegExp(`${PAGE.replace(/[/#]/g, "\\$&")}$`));
  } finally {
    await server.dispose();
  }
});

test("the connection and the login pages have no violation axe can find", async ({
  page,
  server,
}) => {
  await page.goto(server.url + PAGE);
  await expect(said(page, "lease")).toContainText("no accounts");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  const secured = await withAccounts();
  try {
    await page.goto(secured.url + PAGE);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Log in to OScope");
    await expectAccessible(page);
  } finally {
    await secured.dispose();
  }
});

test("the connection and login pages look as their baselines do @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.goto(server.url + PAGE);
  await expect(said(page, "connection")).toContainText("connected");
  await expect(said(page, "connection")).not.toContainText("not read yet");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`connection-${scheme}.png`, { fullPage: true });
    await shot(page, `W1.3-connection-${scheme}`);
  }
  const secured = await withAccounts();
  try {
    await page.goto(secured.url + PAGE);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Log in to OScope");
    await page.getByRole("textbox", { name: "Name" }).blur();
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expect(page).toHaveScreenshot(`login-${scheme}.png`, { fullPage: true });
      await shot(page, `W1.3-login-${scheme}`);
    }
  } finally {
    await secured.dispose();
  }
});
