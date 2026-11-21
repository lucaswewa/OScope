// Covers: web/src/features/dev
import type { Locator, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

/** The page of overlays, and what it says one of its overlays last did. */
async function open(page: Page, url: string): Promise<(name: string) => Locator> {
  await page.goto(`${url}/#/dev/gallery/overlays`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  return (name) => page.locator(`[data-said="${name}"]`);
}

/** Stops the page's clock, to move it on by hand: notices then go only when told. */
async function stopTime(page: Page): Promise<void> {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1000);
}

test("a dialog keeps the focus, holds the page still, and gives the focus back @main", async ({
  page,
  server,
}) => {
  const said = await open(page, server.url);
  const keys = page.keyboard;
  const opener = page.getByRole("button", { name: "New scan…" });
  const dialog = page.getByRole("dialog", { name: "A new scan" });
  const name = dialog.getByRole("textbox", { name: "Name" });
  const project = dialog.getByRole("combobox", { name: "Project" });
  const close = dialog.getByRole("button", { name: "Close" });
  expect(await page.evaluate(() => document.documentElement.scrollHeight > innerHeight)).toBe(true);

  // Opened from the keyboard, with the focus on the first thing in it.
  await opener.focus();
  await keys.press("Enter");
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveAccessibleDescription(
    "Its name and its project can be changed later.",
  );
  await expect(name).toBeFocused();

  // Tab goes round inside it, and Shift+Tab back.
  for (const next of [
    project,
    dialog.getByRole("button", { name: "Cancel" }),
    dialog.getByRole("button", { name: "Create" }),
    close,
    name,
  ]) {
    await keys.press("Tab");
    await expect(next).toBeFocused();
  }
  await keys.press("Shift+Tab");
  await expect(close).toBeFocused();
  await keys.press("Tab");

  // The page behind stays where it is.
  await page.mouse.move(10, 10);
  await page.mouse.wheel(0, 400);
  await page.mouse.wheel(0, 400);
  expect(await page.evaluate(() => window.scrollY)).toBe(0);

  // A list in it opens over it; Escape closes the list, then the dialog.
  await keys.press("Tab");
  await keys.press("Enter");
  const list = page.getByRole("listbox");
  await expect(list.getByRole("option", { name: "Histology" })).toBeFocused();
  const option = await list.getByRole("option", { name: "Cytology" }).boundingBox();
  const topmost = await page.evaluate(
    ([x, y]) => document.elementFromPoint(x!, y!)?.closest("[role='option']")?.textContent?.trim(),
    [option!.x + option!.width / 2, option!.y + option!.height / 2],
  );
  expect(topmost).toBe("Cytology");
  // The list moves the focus a moment after the key: wait for it, as a hand would.
  await keys.press("ArrowDown");
  await expect(list.getByRole("option", { name: "Cytology" })).toBeFocused();
  await keys.press("Enter");
  await expect(list).toBeHidden();
  await expect(project).toHaveText("Cytology");
  await expect(dialog).toBeVisible();
  await keys.press("Enter");
  await expect(list).toBeVisible();
  await keys.press("Escape");
  await expect(list).toBeHidden();
  await expect(dialog).toBeVisible();
  await keys.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();

  // Done by its button; and closed by its own close button.
  await keys.press("Enter");
  await name.fill("slide-4");
  await dialog.getByRole("button", { name: "Create" }).click();
  await expect(dialog).toBeHidden();
  await expect(said("dialog")).toHaveText("Created “slide-4” in cytology.");
  await expect(opener).toBeFocused();
  await keys.press("Enter");
  await expect(name).toBeFocused();
  await keys.press("Shift+Tab");
  await expect(close).toBeFocused();
  await keys.press("Enter");
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();
});

test("a confirmation starts on the answer that changes nothing, and is typed when it can't be undone @main", async ({
  page,
  server,
}) => {
  const said = await open(page, server.url);
  const keys = page.keyboard;
  const question = page.getByRole("alertdialog");

  const stop = page.getByRole("button", { name: "Stop the scan…" });
  await stop.focus();
  await keys.press("Enter");
  await expect(question).toHaveAccessibleName("Stop the scan?");
  await expect(question.getByRole("button", { name: "Keep scanning" })).toBeFocused();
  // A press beside it is no answer: it stays.
  await page.mouse.click(5, 5);
  await expect(question).toBeVisible();
  // Escape is "no", and the focus goes back.
  await keys.press("Escape");
  await expect(question).toBeHidden();
  await expect(said("question")).toHaveText("The scan goes on.");
  await expect(stop).toBeFocused();
  // Tab to "yes", and Enter.
  await keys.press("Enter");
  await expect(question.getByRole("button", { name: "Keep scanning" })).toBeFocused();
  await keys.press("Tab");
  await expect(question.getByRole("button", { name: "Stop the scan" })).toBeFocused();
  await keys.press("Enter");
  await expect(said("question")).toHaveText("The scan was stopped.");
  await expect(stop).toBeFocused();

  // What can't be undone: the focus starts in the field for its word.
  const deletion = page.getByRole("button", { name: "Delete the project…" });
  await deletion.focus();
  await keys.press("Enter");
  const word = question.getByRole("textbox", { name: "Type slide-4 to go on" });
  const go = question.getByRole("button", { name: "Delete the project" });
  await expect(word).toBeFocused();
  await expect(go).toBeDisabled();
  await keys.type("slide-");
  await expect(go).toBeDisabled();
  await keys.type("4");
  await expect(go).toBeEnabled();
  await keys.press("Tab");
  await keys.press("Tab");
  await expect(go).toBeFocused();
  await keys.press("Enter");
  await expect(question).toBeHidden();
  await expect(said("deletion")).toHaveText("The project was deleted.");
  await expect(deletion).toBeFocused();

  // Asked again, the word must be typed again; and Cancel says no.
  await keys.press("Enter");
  await expect(word).toHaveValue("");
  await expect(go).toBeDisabled();
  await keys.press("Tab");
  await keys.press("Enter");
  await expect(question).toBeHidden();
  await expect(said("deletion")).toHaveText("Nothing was deleted.");
  await expect(deletion).toBeFocused();
});

test("a popover and the menus work from the keyboard @main", async ({ page, server }) => {
  const said = await open(page, server.url);
  const keys = page.keyboard;

  // A popover: the focus goes in, and back on Escape.
  const options = page.getByRole("button", { name: "Options" });
  await options.focus();
  await keys.press("Enter");
  const panel = page.getByRole("dialog", { name: "Options" });
  await expect(panel).toBeVisible();
  await expect(panel.getByRole("switch", { name: "Gone in a moment" })).toBeFocused();
  await keys.press("Escape");
  await expect(panel).toBeHidden();
  await expect(options).toBeFocused();

  // A menu: the arrows, past what is switched off, Home, End, and a letter.
  const actions = page.getByRole("button", { name: "Actions" });
  const menu = page.getByRole("menu");
  const item = (name: string): Locator => menu.getByRole("menuitem", { name });
  await actions.focus();
  await keys.press("Enter");
  await expect(item("View")).toBeFocused();
  await keys.press("ArrowDown");
  await expect(item("Download")).toBeFocused();
  await expect(item("Rename")).toBeDisabled();
  await keys.press("ArrowDown");
  await expect(item("Delete")).toBeFocused();
  await keys.press("Home");
  await expect(item("View")).toBeFocused();
  await keys.press("End");
  await expect(item("Delete")).toBeFocused();
  await keys.press("d");
  await expect(item("Download")).toBeFocused();
  await keys.press("Enter");
  await expect(menu).toBeHidden();
  await expect(said("menu")).toHaveText("download");
  await expect(actions).toBeFocused();
  // Escape closes it, with nothing done.
  await keys.press("ArrowDown");
  await expect(item("View")).toBeFocused();
  await keys.press("Escape");
  await expect(menu).toBeHidden();
  await expect(actions).toBeFocused();
  await expect(said("menu")).toHaveText("download");

  // A context menu, from the keyboard on what it is for.
  const target = page.getByText("Press the other button here");
  await target.focus();
  await keys.press("Shift+F10");
  await expect(item("View")).toBeFocused();
  await keys.press("ArrowDown");
  await expect(item("Download")).toBeFocused();
  await keys.press("Enter");
  await expect(menu).toBeHidden();
  await expect(said("context")).toHaveText("download");
  await expect(target).toBeFocused();

  // And where the pointer is: there, the menu itself has the focus.
  await target.click({ button: "right", position: { x: 40, y: 20 } });
  await expect(menu).toBeFocused();
  const at = await menu.boundingBox();
  const box = await target.boundingBox();
  expect(Math.abs(at!.x - (box!.x + 40))).toBeLessThan(8);
  await keys.press("Escape");
  await expect(menu).toBeHidden();
});

test("notices queue, stay while looked at, and leave the rest of the page's Escape alone @main", async ({
  page,
  server,
}) => {
  await page.clock.install();
  await open(page, server.url);
  await stopTime(page);
  const keys = page.keyboard;
  const region = page.getByRole("region", { name: "Notifications (F8)" });
  const notices = region.getByRole("listitem");
  const raise = (name: string) => page.getByRole("button", { name, exact: true }).click();

  for (const name of ["Tell", "Succeed", "Warn", "Fail"]) await raise(name);
  await expect(notices).toHaveText([
    "The stage is at its home. (1)",
    "Saved to the gallery. (2)",
    "The disk is nearly full. (3)",
  ]);
  // Each is read out as it shows: the warning at once.
  await expect(region.getByRole("status")).toHaveText(
    "Notification: The stage is at its home. (1)Notification: Saved to the gallery. (2)",
  );
  await expect(region.getByRole("alert")).toHaveText("Notification: The disk is nearly full. (3)");

  // Time passes: what tells and succeeds goes, and what waited shows.
  await page.clock.runFor(4000);
  await expect(notices).toHaveCount(3);
  await page.clock.runFor(1500);
  await expect(notices).toHaveText([
    "The disk is nearly full. (3)",
    "The camera stopped answering. (4)",
  ]);
  await expect(region.getByRole("alert")).toContainText("The camera stopped answering. (4)");

  // While the pointer is on them, no time passes for them.
  await notices.first().hover();
  await page.clock.runFor(60000);
  await expect(notices).toHaveCount(2);
  await page.mouse.move(5, 5);
  await page.clock.runFor(6000);
  // The warning's ten seconds ran out; what failed stays until sent away.
  await expect(notices).toHaveText(["The camera stopped answering. (4)"]);

  // Escape elsewhere on the page sends nothing away.
  await page.getByRole("button", { name: "Tell", exact: true }).focus();
  await keys.press("Escape");
  await expect(notices).toHaveCount(1);

  // F8 takes the focus to them, and nothing runs out while it is there.
  const tell = page.getByRole("button", { name: "Tell", exact: true });
  await raise("Succeed");
  await tell.focus();
  await keys.press("F8");
  await expect(page.locator(".ui-toasts")).toBeFocused();
  await page.clock.runFor(60000);
  await expect(notices).toHaveCount(2);
  // Escape sends away the one with the focus, then the next; then the focus goes back.
  await keys.press("Tab");
  await expect(notices.first().getByRole("button", { name: "Dismiss" })).toBeFocused();
  await keys.press("Escape");
  await expect(notices).toHaveText(["Saved to the gallery. (5)"]);
  await keys.press("Tab");
  await keys.press("Escape");
  await expect(notices).toHaveCount(0);
  await expect(tell).toBeFocused();

  // What a notice offers to do, done by its button. Time runs again first:
  // Vue tells a press from the one that drew its button by the clock, and a
  // stopped clock gives them the same time.
  await page.clock.resume();
  await raise("With something to do");
  await notices.getByRole("button", { name: "Undo" }).click();
  await expect(notices).toHaveText(["The stage is at its home. (7)"]);
});

test("each overlay passes axe while it is open, in both themes", async ({ page, server }) => {
  await open(page, server.url);
  const keys = page.keyboard;
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);

    await page.getByRole("button", { name: "New scan…" }).click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expectAccessible(page, "[role=dialog]");
    await keys.press("Escape");

    await page.getByRole("button", { name: "Delete the project…" }).click();
    await expect(page.getByRole("alertdialog")).toBeVisible();
    await expectAccessible(page, "[role=alertdialog]");
    await keys.press("Escape");

    await page.getByRole("button", { name: "Options" }).click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expectAccessible(page, "[role=dialog]");
    await keys.press("Escape");

    await page.getByRole("button", { name: "Actions" }).click();
    await expect(page.getByRole("menu")).toBeVisible();
    await expectAccessible(page, "[role=menu]");
    await keys.press("Escape");
    await expect(page.getByRole("menu")).toBeHidden();

    // Notices showing, with the page in use.
    for (const name of ["Tell", "Warn", "Fail"]) {
      await page.getByRole("button", { name, exact: true }).click();
    }
    await expect(page.locator(".ui-toast")).toHaveCount(3);
    await expectAccessible(page);
    if (scheme === "light") await shot(page, "W0.4-overlays-notices");
    while ((await page.locator(".ui-toast").count()) > 0) {
      await page.locator(".ui-toast").first().getByRole("button", { name: "Dismiss" }).click();
    }
  }
});

test("the overlays look as their baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.clock.install();
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`overlays-${scheme}.png`, { fullPage: true });

    await page.getByRole("button", { name: "New scan…" }).click();
    await expect(page.getByRole("textbox", { name: "Name" })).toBeFocused();
    await expect(page).toHaveScreenshot(`overlays-dialog-${scheme}.png`);
    if (scheme === "dark") await shot(page, "W0.4-overlays-dialog-dark");
    await page.keyboard.press("Escape");

    // From the keyboard, which puts the focus on its first entry.
    await page.getByRole("button", { name: "Actions" }).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("menuitem", { name: "View" })).toBeFocused();
    await expect(page).toHaveScreenshot(`overlays-menu-${scheme}.png`);
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toBeHidden();
  }

  // The notices, with time stopped so that they stay. In light, the first
  // three raised; in dark, once the first two's time is up, the last two:
  // every level is in a picture.
  await stopTime(page);
  for (const name of ["Tell", "Succeed", "Warn", "Fail"]) {
    await page.getByRole("button", { name, exact: true }).click();
  }
  const notices = page.locator(".ui-toasts");
  await page.mouse.move(5, 5);
  await inScheme(page, "light");
  await expect(notices).toHaveScreenshot("overlays-notices-light.png");
  await page.clock.runFor(5000);
  await expect(page.locator(".ui-toast")).toHaveCount(2);
  await inScheme(page, "dark");
  await expect(notices).toHaveScreenshot("overlays-notices-dark.png");
});
