// Covers: web/src/features/settings
// Accounts (W8.6, ADR-0055): on an open server, the first account added,
// security said to wait for the next start, the restart, and the login it
// then asks for; on a secured one, with the lease, an account added and
// another removed, a name refused before it is sent, and one's own password
// changed with the old one; axe; the view's baseline; and the notes' shots.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { Server } from "./server.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const ANA = ["ana", "ana's password"] as const;
const BEN = ["ben", "ben's password"] as const;

/** Settings' accounts view. */
async function accounts(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/settings?view=accounts`);
}

/** Logs in on the login the page was sent to, then opens the accounts view. */
async function logIn(
  page: Page,
  url: string,
  [name, password]: readonly [string, string],
): Promise<void> {
  const heading = page.getByRole("heading", { level: 1 });
  await expect(heading).toHaveText("Log in to OScope");
  await page.getByRole("textbox", { name: "Name" }).fill(name);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(heading).not.toHaveText("Log in to OScope");
  await accounts(page, url);
  await expect(page.getByRole("tab", { name: "Accounts", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
}

/** A new account's form, filled. */
async function filled(page: Page, name: string, password: string): Promise<void> {
  const form = said(page, "new-account");
  await form.getByRole("textbox", { name: "Its name" }).fill(name);
  await form.getByLabel("Its password").fill(password);
  await form.getByLabel("The password again").fill(password);
}

/** A server of the test's own, with `ana` and `ben`: the caller disposes of it. */
async function secured(): Promise<Server> {
  const server = new Server();
  server.addAccount(...ANA);
  server.addAccount(...BEN);
  await server.start();
  return server;
}

test.describe("a server that restarts", () => {
  test.use({ restarts: true });

  test("the first account turns security on at the restart, which then asks for the login @main", async ({
    page,
    server,
  }) => {
    test.setTimeout(150_000);
    await accounts(page, server.url);
    await expect(said(page, "open-server")).toContainText("This server has no accounts");
    await filled(page, ...ANA);
    await said(page, "new-account").getByRole("button", { name: "Add it" }).click();
    await expect(said(page, "accounts-said")).toHaveText("ana is added.");
    await expect(said(page, "account-names")).toContainText("ana");
    await expect(said(page, "accounts-restart")).toContainText(
      "Security turns on at the next start.",
    );
    // Restarted, as Power does it: the server asks for the login, and ana is in.
    const on = new Api(server.url);
    await act("/api/v1/power/restart", { confirm: true, force: false }, { on }).catch(
      () => undefined,
    );
    await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
    const answers = () =>
      page.request.get(`${server.url}/health`).then(
        (reply) => reply.ok(),
        () => false,
      );
    await expect.poll(answers, { timeout: 60_000 }).toBe(true);
    // Opened afresh, as a browser opening it would: the app asks the server again.
    await accounts(page, server.url);
    await page.reload();
    await logIn(page, server.url, ANA);
    await expect(page.locator('[data-account="ana"]')).toContainText("You");
    await expect(said(page, "accounts-restart")).toHaveCount(0);
    await expect(said(page, "sessions")).toContainText("ana, idle for");
    await expect(said(page, "idle-times")).toContainText(
      "Sessions end after 8 h idle; the control lease goes after 10 min.",
    );
  });
});

test("with the lease, an account added and another removed; one's own password changed with the old one", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const server = await secured();
  try {
    await accounts(page, server.url);
    await logIn(page, server.url, ANA);
    await page.getByRole("button", { name: "Take control" }).click();
    await expect(said(page, "read-only")).toBeHidden();
    // A name the server wouldn't take: said at its field, and nothing sent.
    await filled(page, "Cy Smith", "cy's password");
    const form = said(page, "new-account");
    await expect(form).toContainText("Lower-case letters, digits");
    await expect(form.getByRole("button", { name: "Add it" })).toBeDisabled();
    await filled(page, "cy", "cy's password");
    await form.getByRole("button", { name: "Add it" }).click();
    await expect(said(page, "accounts-said")).toHaveText("cy is added.");
    // Ben removed, asked about first.
    await page.locator('[data-account="ben"]').getByRole("button", { name: "Remove" }).click();
    await expect(page.getByRole("alertdialog")).toContainText("Remove ben?");
    await page.getByRole("alertdialog").getByRole("button", { name: "Remove" }).click();
    await expect(said(page, "accounts-said")).toHaveText("ben is removed.");
    await expect(said(page, "account-names")).not.toContainText("ben");
    const login = (name: string, password: string) =>
      page.request.post(`${server.url}/api/v1/auth/login`, { data: { name, password } });
    expect((await login(...BEN)).status()).toBe(401);
    expect((await login("cy", "cy's password")).status()).toBe(200);
    // Ana's own password: the old one wrong, refused; right, changed.
    const own = said(page, "own-password");
    await expect(own).toContainText("Your password, ana");
    await own.getByLabel("The one you have").fill("not it");
    await own.getByLabel("A new one").fill("ana's new password");
    await own.getByLabel("The password again").fill("ana's new password");
    await own.getByRole("button", { name: "Change it" }).click();
    await expect(said(page, "accounts-said")).toContainText("the old password isn't right");
    await own.getByLabel("The one you have").fill(ANA[1]);
    await own.getByRole("button", { name: "Change it" }).click();
    await expect(said(page, "accounts-said")).toHaveText("Your password is changed.");
    expect((await login(...ANA)).status()).toBe(401);
    expect((await login("ana", "ana's new password")).status()).toBe(200);
  } finally {
    await server.dispose();
  }
});

test("the accounts view has no violation axe can find, in either theme", async ({ page }) => {
  const server = await secured();
  try {
    await accounts(page, server.url);
    await logIn(page, server.url, ANA);
    await expect(said(page, "sessions")).toContainText("ana, idle for");
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectAccessible(page);
    }
  } finally {
    await server.dispose();
  }
});

test("the accounts view looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await accounts(page, server.url);
  await expect(said(page, "open-server")).toContainText("This server has no accounts");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`accounts-${scheme}.png`);
  }
});

test("the accounts view, for the notes' screenshots", async ({ page }) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  const server = await secured();
  try {
    await page.setViewportSize({ width: 1280, height: 1000 });
    await accounts(page, server.url);
    await logIn(page, server.url, ANA);
    await page.getByRole("button", { name: "Take control" }).click();
    await expect(said(page, "sessions")).toContainText("ana, idle for");
    await expect(said(page, "idle-times")).toContainText("Its holder has been idle for", {
      timeout: 15_000,
    });
    await page.mouse.move(5, 5);
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await shot(page, `W8.6-accounts-${scheme}`);
    }
  } finally {
    await server.dispose();
  }
});
