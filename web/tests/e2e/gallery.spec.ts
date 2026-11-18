import { expect, shot, test } from "./fixtures.ts";

// The page's surface, as the tokens give it to each scheme.
const light = "rgb(255, 255, 255)";
const dark = "rgb(31, 31, 31)";

test("the gallery follows the theme and the accent, and the browser keeps them @main", async ({
  page,
  server,
}) => {
  const asked: string[] = [];
  page.on("request", (request) => asked.push(request.resourceType()));
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto(`${server.url}/#/dev/gallery`);
  const root = page.locator("html");
  const body = page.locator("body");
  const scheme = page.getByRole("status");
  const accent = page.locator('[data-token="accent"]');
  const title = page.getByRole("heading", { level: 1 });

  // As the computer is: light, then dark, with nothing reloaded. A title is
  // in the accent's text colour in light and plain in dark: one token, a
  // choice by scheme inside another.
  await expect(root).not.toHaveAttribute("data-theme");
  await expect(body).toHaveCSS("background-color", light);
  await expect(title).toHaveCSS("color", "rgb(180, 29, 115)");
  await expect(scheme).toHaveAttribute("data-scheme", "light");
  await shot(page, "W0.2-gallery-light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(body).toHaveCSS("background-color", dark);
  await expect(title).toHaveCSS("color", "rgb(232, 232, 232)");
  await expect(scheme).toHaveAttribute("data-scheme", "dark");
  await shot(page, "W0.2-gallery-dark");

  // A theme that is chosen holds against the computer's.
  await page.getByLabel("Theme").selectOption("light");
  await expect(root).toHaveAttribute("data-theme", "light");
  await expect(body).toHaveCSS("background-color", light);
  await expect(scheme).toHaveAttribute("data-scheme", "light");

  // The accent, and what is made from it: a browser that couldn't mix colours
  // or pick by scheme would leave the tint with no colour at all.
  await expect(accent).toHaveCSS("background-color", "rgb(206, 34, 131)");
  await page.getByLabel("Accent colour").selectOption("fern");
  await expect(root).toHaveAttribute("data-accent", "fern");
  await expect(accent).toHaveCSS("background-color", "rgb(79, 121, 66)");
  await expect(page.locator('[data-token="accent-tint"]')).not.toHaveCSS(
    "background-color",
    "rgba(0, 0, 0, 0)",
  );

  // The next visit starts with both.
  await page.reload();
  await expect(root).toHaveAttribute("data-theme", "light");
  await expect(root).toHaveAttribute("data-accent", "fern");
  await expect(page.getByLabel("Theme")).toHaveValue("light");
  await expect(accent).toHaveCSS("background-color", "rgb(79, 121, 66)");

  // The icons are in the bundle: no image was fetched to draw them, and no font.
  await expect(page.locator("svg.icon").first()).toBeVisible();
  expect(asked.filter((kind) => kind === "image" || kind === "font")).toEqual([]);
});

test("the gallery looks as its baselines do, in both themes @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(`${server.url}/#/dev/gallery`);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page).toHaveScreenshot(`gallery-${scheme}.png`, { fullPage: true });
  }
});
