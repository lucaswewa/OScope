import type { Locator, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

/** The page of inputs, and what it says one of its controls holds now. */
async function open(page: Page, url: string): Promise<(name: string) => Locator> {
  await page.goto(`${url}/#/dev/gallery/inputs`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  return (name) => page.locator(`[data-value="${name}"]`);
}

/**
 * Every text an element shows from now on, in turn: for what would flicker
 * past a single look.
 */
async function texts(element: Locator): Promise<() => Promise<string[]>> {
  const seen = await element.evaluateHandle((node) => {
    const all = [node.textContent?.trim() ?? ""];
    new MutationObserver(() => {
      const text = node.textContent?.trim() ?? "";
      if (all.at(-1) !== text) all.push(text);
    }).observe(node, { childList: true, subtree: true, characterData: true });
    return all;
  });
  return () => seen.jsonValue();
}

test("buttons and choices work from the keyboard @main", async ({ page, server }) => {
  const now = await open(page, server.url);
  const keys = page.keyboard;

  // A button: Enter presses it, and so does Space.
  await page.getByRole("button", { name: "Move", exact: true }).focus();
  await keys.press("Enter");
  await keys.press("Space");
  await expect(now("presses")).toHaveText("Pressed 2 times");

  // A busy button keeps the focus, says it is busy, and takes no press.
  await page.getByRole("switch", { name: "Autofocus is under way" }).focus();
  await keys.press("Space");
  const busy = page.locator("button[aria-busy='true']");
  await expect(busy).toHaveAccessibleName("Working… Autofocus");
  await busy.focus();
  await keys.press("Enter");
  await keys.press("Space");
  await expect(busy).toBeFocused();
  await expect(now("presses")).toHaveText("Pressed 2 times");

  // A button that stays pressed.
  const grid = page.getByRole("button", { name: "Grid", exact: true }).first();
  await expect(grid).toHaveAttribute("aria-pressed", "true");
  await grid.focus();
  await keys.press("Space");
  await expect(grid).toHaveAttribute("aria-pressed", "false");
  await expect(now("grid")).toHaveText("Now: false");

  // A group of them: the arrows move between its buttons, and one stays pressed.
  const objective = page.getByRole("group", { name: "Objective" });
  await objective.getByRole("button", { name: "20×" }).focus();
  await keys.press("ArrowRight");
  await expect(objective.getByRole("button", { name: "40×" })).toBeFocused();
  await keys.press("Space");
  await expect(now("objective")).toHaveText("Now: 40");
  await keys.press("Space");
  await expect(now("objective")).toHaveText("Now: 40");
  await keys.press("Home");
  await expect(objective.getByRole("button", { name: "4×" })).toBeFocused();

  // With several pressed at once.
  const overlays = page.getByRole("group", { name: "Overlays" });
  await overlays.getByRole("button", { name: "Scale bar" }).focus();
  await keys.press("ArrowRight");
  await keys.press("Enter");
  await expect(now("overlays")).toHaveText("Now: scale, grid");

  // A checkbox: Space ticks it, and a press on its words is a press on it, once.
  const invert = page.getByRole("checkbox", { name: "Invert x" });
  await invert.focus();
  await keys.press("Space");
  await expect(invert).toBeChecked();
  await expect(now("invert")).toHaveText("Now: true");
  await page.getByText("Invert x", { exact: true }).click();
  await expect(invert).not.toBeChecked();
  await expect(now("invert")).toHaveText("Now: false");

  // Read-only: reached, and not changed.
  const fixed = page.getByRole("checkbox", { name: "Set by the instrument" });
  await fixed.focus();
  await expect(fixed).toBeFocused();
  await keys.press("Space");
  await expect(fixed).toBeChecked();

  // Radio buttons: an arrow moves the choice, past the one that is switched off.
  const order = page.getByRole("radiogroup", { name: "Scan order" });
  await order.getByRole("radio", { name: "Snake" }).focus();
  // The key only once the focus is there: a busy Firefox took it before (W3.2).
  await expect(order.getByRole("radio", { name: "Snake" })).toBeFocused();
  await keys.press("ArrowDown");
  await expect(order.getByRole("radio", { name: "Raster" })).toBeChecked();
  await expect(order.getByRole("radio", { name: "Raster" })).toBeFocused();
  await expect(now("order")).toHaveText("Now: raster");
  await keys.press("ArrowDown");
  await expect(order.getByRole("radio", { name: "Snake" })).toBeChecked();
  await expect(order.getByRole("radio", { name: "Spiral" })).toBeDisabled();

  // A switch.
  const live = page.getByRole("switch", { name: "Live image" });
  await live.focus();
  await keys.press("Space");
  await expect(live).toBeChecked();
  await expect(now("live")).toHaveText("Now: true");
  await keys.press("Enter");
  await expect(now("live")).toHaveText("Now: false");
});

test("text, numbers and sliders work from the keyboard @main", async ({ page, server }) => {
  const now = await open(page, server.url);
  const keys = page.keyboard;

  // Text is held as it is typed.
  await page.getByRole("textbox", { name: "Sample ID" }).fill("slide 4");
  await expect(now("name")).toHaveText("Now: slide 4");
  await expect(page.getByRole("textbox", { name: "Serial number" })).not.toBeEditable();
  await expect(page.getByRole("textbox", { name: "Operator" })).toBeDisabled();
  // A field's error describes its control.
  await expect(page.getByRole("textbox", { name: "Address" })).toHaveAccessibleDescription(
    "This isn't an address the app can reach.",
  );

  // A number isn't taken until Enter, and one that isn't a number is refused, with why.
  const x = page.getByRole("spinbutton", { name: "x", exact: true });
  await x.fill("12,5");
  await expect(now("x")).toHaveText("Now: 1250");
  await keys.press("Enter");
  await expect(page.getByRole("main").getByRole("alert")).toHaveText("This isn't a number.");
  await expect(x).toHaveAttribute("aria-invalid", "true");
  await expect(x).toHaveAccessibleDescription("This isn't a number.");
  await expect(now("x")).toHaveText("Now: 1250");
  await keys.press("Escape");
  await expect(x).toHaveValue("1250");
  await expect(page.getByRole("main").getByRole("alert")).toHaveCount(0);

  // Taken on Enter; then the arrows, the page keys, and the range's ends.
  await x.fill("300");
  await keys.press("Enter");
  await expect(now("x")).toHaveText("Now: 300");
  await keys.press("ArrowUp");
  await expect(now("x")).toHaveText("Now: 310");
  await keys.press("PageDown");
  await expect(now("x")).toHaveText("Now: 210");
  await keys.press("End");
  await expect(now("x")).toHaveText("Now: 5000");
  await keys.press("ArrowUp");
  await expect(x).toHaveValue("5000");
  await keys.press("Home");
  await expect(now("x")).toHaveText("Now: -5000");

  // Taken on leaving the field, too; and a value outside the range is refused.
  const overlap = page.getByRole("spinbutton", { name: "Image overlap" });
  await overlap.fill("0.5");
  await keys.press("Tab");
  await expect(now("overlap")).toHaveText("Now: 0.5");
  await expect(overlap).toHaveValue("0.50");
  await overlap.fill("0.9");
  await keys.press("Tab");
  await expect(page.getByRole("main").getByRole("alert")).toHaveText("At most 0.7.");
  await expect(now("overlap")).toHaveText("Now: 0.5");
  await expect(overlap).toHaveAccessibleDescription("At most 0.7. From 0.1 to 0.7.");

  // No value at all, where the field is optional.
  const delay = page.getByRole("spinbutton", { name: "Delay" });
  await delay.fill("3");
  await keys.press("Enter");
  await expect(now("delay")).toHaveText("Now: 3");
  await delay.fill("");
  await keys.press("Enter");
  await expect(now("delay")).toHaveText("Now: nothing");

  // The wheel moves the field with the focus, and the page stays put; without it, nothing.
  const step = page.getByRole("spinbutton", { name: "Step" });
  await step.hover();
  await page.mouse.wheel(0, -100);
  await expect(now("step")).toHaveText("Now: 0.5");
  await step.focus();
  await step.hover();
  const scrolled = await page.evaluate(() => window.scrollY);
  await page.mouse.wheel(0, -100);
  await expect(now("step")).toHaveText("Now: 0.6");
  await page.mouse.wheel(0, 100);
  await page.mouse.wheel(0, 100);
  await expect(now("step")).toHaveText("Now: 0.4");
  expect(await page.evaluate(() => window.scrollY)).toBe(scrolled);

  // A slider: the arrows by a step, the ends by Home and End.
  const exposure = page.getByRole("slider", { name: "Exposure" });
  await expect(exposure).toHaveAttribute("aria-valuenow", "20");
  await exposure.focus();
  await keys.press("ArrowRight");
  await expect(now("exposure")).toHaveText("Now: 21");
  await keys.press("ArrowLeft");
  await keys.press("ArrowLeft");
  await expect(now("exposure")).toHaveText("Now: 19");
  await keys.press("End");
  await expect(now("exposure")).toHaveText("Now: 200");
  await keys.press("Home");
  await expect(now("exposure")).toHaveText("Now: 1");

  // And the number beside it moves the thumb.
  const typed = page.getByRole("spinbutton", { name: "Exposure" });
  await expect(typed).toHaveValue("1");
  await typed.fill("50");
  await keys.press("Enter");
  await expect(exposure).toHaveAttribute("aria-valuenow", "50");
  await expect(exposure).toHaveAttribute("aria-valuetext", "50 ms");
  await expect(now("exposure")).toHaveText("Now: 50");
  await expect(page.getByRole("slider", { name: "Brightness" })).toBeDisabled();

  // Read-only: the keyboard reaches the thumb, and no key moves it.
  const lamp = page.getByRole("slider", { name: "Lamp" });
  await lamp.focus();
  await expect(lamp).toBeFocused();
  await keys.press("ArrowRight");
  await keys.press("End");
  await expect(lamp).toHaveAttribute("aria-valuenow", "40");
  await expect(page.getByRole("spinbutton", { name: "Lamp" })).toHaveValue("40");
});

test("lists and tooltips work from the keyboard @main", async ({ page, server }) => {
  const now = await open(page, server.url);
  const keys = page.keyboard;
  const list = page.getByRole("listbox");

  // A list opens on Enter; an arrow moves through it; Enter chooses and closes it.
  const workflow = page.getByRole("combobox", { name: "Workflow" });
  await expect(workflow).toHaveText("Histo scan");
  const shown = await texts(workflow);
  // Measured now: an open list hides the page behind it from a screen reader,
  // and so from a locator that finds things as one does.
  const field = await workflow.boundingBox();
  await workflow.focus();
  await keys.press("Enter");
  await expect(list).toBeVisible();
  await expect(list.getByRole("option")).toHaveText([
    "Histo scan",
    "Grid scan",
    "Counting chamber",
    "Time lapse",
  ]);
  await expect(list.getByRole("option", { name: "Histo scan" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  // The list is drawn at the page's root, where its component's own style
  // doesn't reach: it is a raised surface, at least as wide as its field.
  await expect(list).toHaveCSS("border-top-color", "rgb(196, 196, 196)");
  await expect(list).not.toHaveCSS("box-shadow", "none");
  expect((await list.boundingBox())!.width).toBeGreaterThanOrEqual(field!.width);
  await expect(list.getByRole("option", { name: "Histo scan" })).toBeFocused();
  await keys.press("ArrowDown");
  await expect(list.getByRole("option", { name: "Grid scan" })).toBeFocused();
  await keys.press("Enter");
  await expect(list).toBeHidden();
  await expect(workflow).toHaveText("Grid scan");
  await expect(workflow).toBeFocused();
  await expect(now("workflow")).toHaveText("Now: grid");

  // Escape closes it with the choice as it was; End stops before a choice that is switched off.
  await keys.press("Space");
  await expect(list.getByRole("option", { name: "Grid scan" })).toBeFocused();
  await keys.press("ArrowUp");
  await expect(list.getByRole("option", { name: "Histo scan" })).toBeFocused();
  await keys.press("Escape");
  await expect(list).toBeHidden();
  await expect(now("workflow")).toHaveText("Now: grid");
  await keys.press("ArrowDown");
  await expect(list.getByRole("option", { name: "Grid scan" })).toBeFocused();
  await keys.press("End");
  await expect(list.getByRole("option", { name: "Counting chamber" })).toBeFocused();
  await keys.press("Enter");
  await expect(now("workflow")).toHaveText("Now: chip");

  // A letter finds a choice.
  await keys.press("Enter");
  await expect(list.getByRole("option", { name: "Counting chamber" })).toBeFocused();
  await keys.press("h");
  await expect(list.getByRole("option", { name: "Histo scan" })).toBeFocused();
  await keys.press("Enter");
  await expect(now("workflow")).toHaveText("Now: histo");
  // Through all of it the field showed each choice, and never its placeholder between two.
  expect(await shown()).toEqual(["Histo scan", "Grid scan", "Counting chamber", "Histo scan"]);

  // A list with nothing chosen says so, and a read-only one doesn't open.
  await expect(page.getByRole("combobox", { name: "On capture" })).toHaveText("Choose…");
  const camera = page.getByRole("combobox", { name: "Camera" }).first();
  await camera.focus();
  await expect(camera).toBeFocused();
  await keys.press("Enter");
  await keys.press("ArrowDown");
  await expect(list).toBeHidden();
  await expect(page.getByRole("combobox", { name: "Camera" }).last()).toBeDisabled();

  // A combobox narrows its list to what is typed.
  const node = page.getByRole("combobox", { name: "Node" });
  await node.focus();
  await keys.type("den");
  await expect(list.getByRole("option")).toHaveText(["denoise.gaussian", "denoise.median"]);
  await keys.press("ArrowDown");
  await keys.press("Enter");
  await expect(list).toBeHidden();
  await expect(node).toHaveValue("denoise.median");
  await expect(now("node")).toHaveText("Now: denoise.median");

  // What matches nothing says so, and leaves the choice as it was.
  await node.fill("zzz");
  await expect(page.getByText("Nothing matches.")).toBeVisible();
  await keys.press("Escape");
  await expect(now("node")).toHaveText("Now: denoise.median");

  // A tooltip shows when the keyboard's focus reaches its control, even when
  // that scrolls the control into view from off the screen.
  const fill = page.getByRole("button", { name: "Fill in" });
  const tip = page.getByRole("tooltip");
  await camera.evaluate((before: HTMLElement) => {
    before.focus({ preventScroll: true });
    window.scrollTo(0, 0);
  });
  await expect(fill).not.toBeInViewport();
  await keys.press("Tab");
  await expect(fill).toBeFocused();
  await expect(fill).toBeInViewport();
  await expect(tip).toHaveText("Fill in where the stage is");
  await expect(fill).toHaveAccessibleDescription("Fill in where the stage is");
  // Drawn at the page's root too: small text on the theme turned over.
  const drawn = page.locator(".ui-tooltip");
  await expect(drawn).toHaveCSS("background-color", "rgb(26, 26, 26)");
  await expect(drawn).toHaveCSS("color", "rgb(255, 255, 255)");

  // Escape sends it away, and the focus stays; leaving and coming back shows it again.
  await keys.press("Escape");
  await expect(tip).toBeHidden();
  await expect(fill).toBeFocused();
  await keys.press("Shift+Tab");
  await expect(camera).toBeFocused();
  await keys.press("Tab");
  await expect(tip).toBeVisible();
  await keys.press("Shift+Tab");
  await expect(tip).toBeHidden();

  // The pointer shows it too, after resting, and it goes when the pointer does.
  await fill.hover();
  await expect(tip).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(tip).toBeHidden();
});

test("a group of buttons, and one of radio buttons, are each one stop for Tab", async ({
  page,
  server,
}) => {
  await open(page, server.url);
  const objective = page.getByRole("group", { name: "Objective" });
  await objective.getByRole("button", { name: "20×" }).focus();
  await page.keyboard.press("Tab");
  // Past 40×, to the next group's own stop.
  await expect(
    page.getByRole("group", { name: "Overlays" }).getByRole("button", { name: "Scale bar" }),
  ).toBeFocused();

  const order = page.getByRole("radiogroup", { name: "Scan order" });
  await order.getByRole("radio", { name: "Snake" }).focus();
  await page.keyboard.press("Tab");
  await expect(
    page.getByRole("radiogroup", { name: "Channel" }).getByRole("radio", { name: "Green" }),
  ).toBeFocused();
});

test("the inputs pass axe, in both themes, and with a list open", async ({ page, server }) => {
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
    await shot(page, `W0.3-inputs-${scheme}`);
  }
  // In error, and with its list open.
  const x = page.getByRole("spinbutton", { name: "x", exact: true });
  await x.fill("12,5");
  await x.press("Enter");
  await expectAccessible(page);
  await page.getByRole("combobox", { name: "Workflow" }).click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await expectAccessible(page, "[role=listbox]");
  await page.keyboard.press("Escape");
  await page.getByRole("combobox", { name: "Node" }).click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await expectAccessible(page, "[role=listbox]");
});

test("the inputs look as their baselines do, in both themes @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`inputs-${scheme}.png`, { fullPage: true });
  }
});
