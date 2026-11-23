// Covers: web/src/features/workspace
// Reports (W7.9, P47): one made on the results chosen and shown, as HTML in
// a sandboxed frame and as PDF, its files to download; a template changed,
// previewed and saved, changing the next report, refused where it is wrong,
// and deleted; the HTML's frames sandboxed, a template's script not run; axe.
import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { counted } from "./reports.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const COUNT = "Discs counted";
const STAINS = "Stains separated: H-DAB, and DAB scored";

/** The Reports tab, the discs' project chosen. */
async function opened(page: Page, url: string): Promise<void> {
  await counted(url);
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Reports" }).click();
  await said(page, "reports").getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: "Discs", exact: true }).click();
  await expect(said(page, "make").getByRole("checkbox", { name: COUNT })).toBeChecked();
}

/** A report made through `template`, and shown. */
async function made(page: Page, template = "standard"): Promise<void> {
  const make = said(page, "make");
  await make.getByRole("combobox", { name: "The template" }).click();
  await page.getByRole("option", { name: template, exact: true }).click();
  await make.getByRole("button", { name: "Make it" }).click();
  await expect(said(page, "making")).toHaveText("Made: it is shown.", { timeout: 30_000 });
}

/** The template editor, its HTML changed by `change`, saved as `name`. */
async function edited(page: Page, name: string, change: (html: string) => string) {
  const templates = said(page, "templates");
  await said(page, "reports").getByRole("tab", { name: "Templates" }).click();
  const html = templates.getByRole("textbox", { name: "Its HTML, for minijinja" });
  await expect(html).toHaveValue(/\{\{ report\.title \}\}<\/h1>/);
  await html.fill(change(await html.inputValue()));
  await templates.getByRole("textbox", { name: "Saved as" }).fill(name);
  return templates;
}

test("a report is made on the results chosen, shown as HTML and PDF, its files to download @main", async ({
  page,
  server,
}) => {
  await opened(page, server.url);
  const make = said(page, "make");
  await expect(make.getByRole("checkbox", { name: STAINS })).toBeChecked();
  // Neither: it can't be made.
  await make.getByRole("checkbox", { name: COUNT }).click();
  await make.getByRole("checkbox", { name: STAINS }).click();
  await expect(said(page, "cannot")).toHaveText("Choose at least one result.");
  await expect(make.getByRole("button", { name: "Make it" })).toBeDisabled();
  // The count alone.
  await make.getByRole("checkbox", { name: COUNT }).click();
  await made(page);
  const read = said(page, "read");
  await expect(read.getByRole("combobox", { name: "The report" })).toHaveText("Discs: results");
  const frame = said(page, "report-html");
  await expect(frame).toHaveAttribute("sandbox", "");
  const shown = page.frameLocator('[data-said="report-html"]');
  await expect(shown.getByRole("heading", { level: 1 })).toHaveText("Discs: results");
  await expect(shown.getByRole("heading", { level: 2, name: COUNT })).toBeVisible();
  await expect(shown.getByRole("heading", { level: 2, name: STAINS })).toHaveCount(0);
  // As PDF: the server's PDF, in the browser's viewer where it has one.
  await read.getByRole("button", { name: "PDF", exact: true }).click();
  const pdf = await page.request.get(
    new URL((await said(page, "report-pdf").getAttribute("data"))!, server.url).href,
  );
  expect(pdf.headers()["content-type"]).toBe("application/pdf");
  expect((await pdf.body()).subarray(0, 5).toString()).toBe("%PDF-");
  // Its files, each to download.
  for (const file of ["report.pdf", "report.html", "model.json"]) {
    await expect(read.locator(`[data-said="file-${file}"]`)).toBeVisible();
  }
});

test("a changed template is previewed, saved, and changes the next report; a wrong one is refused", async ({
  page,
  server,
}) => {
  await opened(page, server.url);
  // A report through OScope's own first: the next, through the user's, is shown in its place.
  await made(page);
  await expect(page.frameLocator('[data-said="report-html"]').locator("h1")).toHaveText(
    "Discs: results",
  );
  const heading = (html: string) =>
    html.replace("<h1>{{ report.title }}</h1>", "<h1>Our lab: {{ report.title }}</h1>");
  const templates = await edited(page, "our-lab", heading);
  // Previewed on the count: nothing kept.
  await templates.getByRole("combobox", { name: "Previewed on" }).click();
  await page.getByRole("option", { name: COUNT, exact: true }).click();
  await templates.getByRole("button", { name: "Preview" }).click();
  const preview = page.frameLocator('[data-said="preview-html"]');
  await expect(preview.getByRole("heading", { level: 1 })).toHaveText("Our lab: Discs: results");
  await expect(said(page, "templates-said")).toHaveText("Previewed: nothing was kept.");
  // Kept nowhere: the templates are still OScope's own alone.
  await templates.getByRole("combobox", { name: "The template" }).click();
  await expect(page.getByRole("option")).toHaveText(["standard"]);
  await page.keyboard.press("Escape");
  // Saved, and the next report made through it.
  await templates.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "templates-said")).toHaveText("Saved as our-lab.");
  await said(page, "reports").getByRole("tab", { name: "Make and read" }).click();
  await made(page, "our-lab");
  await expect(said(page, "read").getByRole("combobox", { name: "The report" })).toHaveText(
    "Discs: results (our-lab)",
  );
  const shown = page.frameLocator('[data-said="report-html"]');
  await expect(shown.getByRole("heading", { level: 1 })).toHaveText("Our lab: Discs: results");
  // A wrong one is refused, saying where; and kept nowhere.
  await edited(page, "wrong", (html) =>
    html.replace("{{ report.title }}</h1>", "{{ report.title </h1>"),
  );
  await templates.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "template-wrong")).toContainText("line");
  // Deleted: the one chosen, still the one saved.
  await expect(templates.getByRole("combobox", { name: "The template" })).toHaveText("our-lab");
  await templates.getByRole("button", { name: "Delete" }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click();
  await expect(said(page, "templates-said")).toHaveText("our-lab was deleted.");
  await templates.getByRole("combobox", { name: "The template" }).click();
  await expect(page.getByRole("option")).toHaveText(["standard"]);
});

test("a report's and a preview's HTML are sandboxed: a template's script doesn't run @main", async ({
  page,
  server,
}) => {
  await opened(page, server.url);
  await page.evaluate(() => {
    const heard: unknown[] = [];
    Object.assign(window, { heard });
    addEventListener("message", (event) => heard.push(event.data));
  });
  const script =
    '<script>document.body.dataset.ran = "yes"; parent.postMessage("ran", "*");</script>';
  const templates = await edited(page, "scripted", (html) =>
    html.replace("</body>", `${script}</body>`),
  );
  await templates.getByRole("button", { name: "Preview" }).click();
  await expect(said(page, "preview-html")).toHaveAttribute("sandbox", "");
  const preview = page.frameLocator('[data-said="preview-html"]');
  await expect(preview.getByRole("heading", { level: 1 })).toHaveText("A sample report");
  await expect(preview.locator("body")).not.toHaveAttribute("data-ran", "yes");
  await templates.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "templates-said")).toHaveText("Saved as scripted.");
  await said(page, "reports").getByRole("tab", { name: "Make and read" }).click();
  await made(page, "scripted");
  await expect(said(page, "report-html")).toHaveAttribute("sandbox", "");
  const shown = page.frameLocator('[data-said="report-html"]');
  await expect(shown.getByRole("heading", { level: 1 })).toHaveText("Discs: results");
  await expect(shown.locator("body")).not.toHaveAttribute("data-ran", "yes");
  // The server sandboxes the page too.
  const page_ = await page.request.get(
    new URL((await said(page, "report-html").getAttribute("src"))!, server.url).href,
  );
  expect(page_.headers()["content-security-policy"]).toBe("sandbox");
  expect(await page.evaluate(() => (window as unknown as { heard: unknown[] }).heard)).toEqual([]);
});

test("the reports and their templates have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await opened(page, server.url);
  await made(page);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await said(page, "reports").getByRole("tab", { name: "Templates" }).click();
  await said(page, "templates").getByRole("button", { name: "Preview" }).click();
  await expect(page.frameLocator('[data-said="preview-html"]').locator("h1")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a report on the count and the DAB measure, and a template's preview, for the notes' screenshots", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  await page.setViewportSize({ width: 1280, height: 1000 });
  await opened(page, server.url);
  await made(page);
  // The job's notice dismissed: it covers the report.
  const toasts = page.getByRole("region", { name: "Notifications (F8)" });
  await toasts.getByRole("button", { name: "Dismiss" }).first().click();
  await expect(toasts.getByRole("listitem")).toHaveCount(0);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.9-reports-${scheme}`);
  }
  const heading = (html: string) =>
    html.replace("<h1>{{ report.title }}</h1>", "<h1>Our lab: {{ report.title }}</h1>");
  const templates = await edited(page, "our-lab", heading);
  await templates.getByRole("combobox", { name: "Previewed on" }).click();
  await page.getByRole("option", { name: COUNT, exact: true }).click();
  await templates.getByRole("button", { name: "Preview" }).click();
  await expect(page.frameLocator('[data-said="preview-html"]').locator("h1")).toHaveText(
    "Our lab: Discs: results",
  );
  await templates.getByRole("combobox", { name: "The template" }).scrollIntoViewIfNeeded();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.9-templates-${scheme}`);
  }
});
