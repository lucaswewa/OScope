import type { APIRequestContext, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

const EXAMPLE = "The gallery's own: a scan's settings";

/** The page of forms, its example drawn, and what it says one of its parts holds now. */
async function open(page: Page, url: string) {
  await page.goto(`${url}/#/dev/gallery/forms`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  const form = page.getByRole("group", { name: EXAMPLE });
  await expect(form).toBeVisible();
  return { form, said: (name: string) => page.locator(`[data-said="${name}"]`) };
}

/** How many schemas the server publishes, counted here from what it serves. */
async function published(request: APIRequestContext, url: string): Promise<number> {
  const read = async (path: string) => (await request.get(`${url}/${path}`)).json();
  const things = (await read("api/v1/directory/things")) as Record<
    string,
    Record<string, Record<string, unknown>>
  >[];
  const nodes = (await read("api/v1/preview/nodes")) as unknown[];
  const patterns = (await read("api/v1/scan/patterns")) as { schema: unknown }[];
  let count = nodes.length + patterns.filter((one) => one.schema).length;
  for (const td of things) {
    count += Object.keys(td.properties ?? {}).length;
    for (const action of Object.values(td.actions ?? {}))
      count += Number("input" in action) + Number("output" in action);
    for (const event of Object.values(td.events ?? {})) count += Number("data" in event);
  }
  return count;
}

test("every schema the server publishes is drawn as a form, and none as JSON", async ({
  page,
  server,
  request,
}) => {
  const { said } = await open(page, server.url);
  const count = await published(request, server.url);
  expect(count).toBeGreaterThan(250);
  await page.getByRole("tab", { name: "Every schema" }).click();
  // A construct new to the server's schemas, which the forms don't draw, fails here.
  await expect(said("every")).toHaveText(`${count} schemas drawn, none as JSON.`);
  await expect(page.locator(".every .schema-form")).toHaveCount(count);
  await expect(page.locator("[data-fallback]")).toHaveCount(0);
});

test("a form is filled in from the keyboard, and says why a value isn't taken @main", async ({
  page,
  server,
}) => {
  const { form, said } = await open(page, server.url);
  const keys = page.keyboard;
  await expect(said("faults")).toHaveText("Faults found: 2.");

  // It starts from the schema's defaults, each shown in its field.
  const exposure = form.getByRole("spinbutton", { name: "Exposure (ms)" });
  await expect(exposure).toHaveValue("20");
  await expect(form.getByRole("spinbutton", { name: "Overlap" })).toHaveValue("0.1");
  await expect(form.getByRole("textbox", { name: "Slide" })).toHaveValue('"AB-1234"');

  // Text, and a number out of range, which isn't taken: the field says why.
  await form.getByRole("textbox", { name: "Name" }).focus();
  await keys.type("slide 4");
  await expect(said("faults")).toHaveText("Faults found: 1.");
  await exposure.focus();
  await keys.press("Control+a");
  await keys.type("5000");
  await keys.press("Enter");
  await expect(form.getByText("At most 1000.")).toBeVisible();
  await expect(said("value")).toContainText('"exposure_ms": 20,');
  await keys.press("Escape");
  await expect(exposure).toHaveValue("20");
  await keys.press("ArrowUp");
  await expect(said("value")).toContainText('"exposure_ms": 21,');

  // A union's kind, chosen from its list: the fields of that kind follow.
  const region = form.getByRole("group", { name: "Region" });
  await region.getByRole("combobox", { name: "Kind" }).focus();
  await keys.press("Enter");
  await expect(page.getByRole("option", { name: "whole" })).toBeFocused();
  await keys.press("ArrowDown");
  await expect(page.getByRole("option", { name: "rect" })).toBeFocused();
  await keys.press("Enter");
  await expect(region.getByRole("spinbutton", { name: "Width" })).toHaveValue("1000");
  await expect(said("value")).toContainText('"rect": {');

  // A value that may be left out, given by its box; and a list's item added.
  await form.getByRole("checkbox", { name: "White balance" }).focus();
  await keys.press("Space");
  await expect(form.getByRole("spinbutton", { name: "Red" })).toHaveValue("1");
  await form.getByRole("button", { name: "Add to Heights (µm)" }).focus();
  await keys.press("Enter");
  await expect(form.getByRole("spinbutton", { name: "Heights (µm) 2" })).toHaveValue("0");
  await expect(said("value")).toContainText('"heights_um": [\n    0,\n    0\n  ]');

  // The project's ID is still wrong, and says so.
  await expect(said("faults")).toHaveText("Faults found: 1.");
  await expect(
    form.getByText("A UUID, such as 3f2c1a9e-8b7d-4c6e-9a1f-2b3c4d5e6f70."),
  ).toBeVisible();
});

test("the server's refusals are put on the fields they name, or on the whole form", async ({
  page,
  server,
}) => {
  const { said } = await open(page, server.url);
  await page.getByRole("combobox", { name: "Schema" }).fill("reorder");
  await page.getByRole("option", { name: "Jobs · reorder · input" }).click();
  const form = page.getByRole("group", { name: "Jobs · reorder · input" });
  const position = form.getByRole("spinbutton", { name: "Position" });
  await expect(position).toHaveValue("0");
  await position.fill("1.5");
  await position.press("Enter");
  await expect(said("faults")).toHaveText("Faults found: 2.");

  /** Sends the value, as it stands with its faults: the server refuses it, and does nothing. */
  async function ask(): Promise<{ loc: unknown[]; msg: string }> {
    const answered = page.waitForResponse((answer) =>
      answer.url().endsWith("/api/v1/jobs/reorder"),
    );
    await page.getByRole("button", { name: "Let the server check it" }).click();
    const answer = await answered;
    expect(answer.status()).toBe(422);
    await expect(said("server")).toHaveText("The server refused it, on the fields it named.");
    return ((await answer.json()) as { detail: { loc: unknown[]; msg: string }[] }).detail[0]!;
  }

  // The server checks the value against its schema first, field by field.
  const whole = await ask();
  expect(whole.loc).toEqual(["body", "position"]);
  await expect(position).toHaveAccessibleDescription(`A whole number. ${whole.msg}`);
  await expect(said("faults")).toHaveText("Faults found: 3.");
  // Changed, the field's refusal goes.
  await position.fill("2");
  await position.press("Enter");
  await expect(position).toHaveAccessibleDescription("");
  await expect(said("faults")).toHaveText("Faults found: 1.");

  // Then it reads the value as the action takes it: a malformed ID names no field.
  const read = await ask();
  expect(read.loc).toEqual(["body"]);
  await expect(form.getByText(read.msg, { exact: true })).toBeVisible();
  await form.getByRole("textbox", { name: "Id" }).fill("3f2c1a9e-8b7d-4c6e-9a1f-2b3c4d5e6f70");
  await expect(said("faults")).toHaveText("No fault found.");
  await expect(form.getByText(read.msg, { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Let the server check it" })).toBeDisabled();
});

test("the forms pass axe, in both themes, and every schema's too", async ({ page, server }) => {
  const { form } = await open(page, server.url);
  await form.getByRole("checkbox", { name: "White balance" }).click();
  await form.getByRole("checkbox", { name: "Labels" }).click();
  await form.getByRole("button", { name: "Add to Labels" }).click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await inScheme(page, "light");
  await page.mouse.move(5, 5);
  await page.evaluate(() => window.scrollTo(0, 0));
  await shot(page, "W0.6-forms-example");
  await page.getByRole("tab", { name: "Every schema" }).click();
  await expect(page.locator('[data-said="every"]')).toBeVisible();
  await expectAccessible(page);
});

test("the forms page looks as its baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  const { form } = await open(page, server.url);
  await expect(page.getByText("Reading the server's schemas…")).toHaveCount(0);
  const region = form.getByRole("group", { name: "Region" });
  await region.getByRole("combobox", { name: "Kind" }).click();
  await page.getByRole("option", { name: "rect" }).click();
  await expect(region.getByRole("spinbutton", { name: "Width" })).toBeVisible();
  // No control keeps the focus, or the pointer, in the picture.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`forms-${scheme}.png`, { fullPage: true });
    if (scheme === "dark") {
      await region.evaluate((element) => element.scrollIntoView({ block: "start" }));
      await shot(page, "W0.6-forms-dark");
    }
  }
});
