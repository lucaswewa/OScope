// The stage's controls (W3.3), on the simulator at real distances: the wait
// to be homed, and homing; a move arriving; steps from the pad and from the
// keys on the image; a step past a limit refused before it is sent; a jog
// moving while held and stopping on release; a long move halted by Escape
// and by Halt; an alarm said and cleared; pointing, a double click centring
// the place within the calibration's error, found again in the image, and a
// drag panning; what pointing lacks without a calibration; axe; and the
// baselines.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { finished, type JobRef } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { found, greys } from "./pixels.ts";
import { clockAt, fast, FAST, homed, overTissue } from "./sim.ts";

const pane = (page: Page) => page.locator('[data-said="stage"]');
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const button = (page: Page, name: string) => pane(page).getByRole("button", { name, exact: true });
const field = (page: Page, name: string) =>
  pane(page).getByRole("spinbutton", { name: new RegExp(`^${name}`) });
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
/** A step to choose, in its group, `xy` or `z`: a button that stays pressed. */
const step = (page: Page, axes: "xy" | "z", name: string) =>
  pane(page)
    .getByRole("group", { name: axes === "xy" ? "Step in X and Y" : "Step in Z" })
    .getByRole("button", { name, exact: true });
const notices = (page: Page) => page.getByRole("region", { name: /Notifications/ });

interface Xyz {
  x: number;
  y: number;
  z: number;
}

/** Where the stage is, and when it said so, in seconds. */
async function sample(url: string): Promise<{ time: number; position: Xyz }> {
  return (await (await fetch(`${url}/api/v1/stage/position`)).json()) as {
    time: number;
    position: Xyz;
  };
}
const position = async (url: string) => (await sample(url)).position;
async function state(url: string): Promise<string> {
  return ((await (await fetch(`${url}/api/v1/stage/status`)).json()) as { state: string }).state;
}
/** How many times the stage's `action` was invoked. */
async function invoked(url: string, action: string): Promise<number> {
  return ((await (await fetch(`${url}/api/v1/stage/${action}`)).json()) as unknown[]).length;
}
/** Whether two places are within `within` µm of each other along each axis. */
const near = (a: Xyz, b: Partial<Xyz>, within = 3) =>
  (["x", "y", "z"] as const).every(
    (axis) => b[axis] === undefined || Math.abs(a[axis] - b[axis]) <= within,
  );

/** Waits until the stage is within 3 µm of `to`, along the axes given. */
async function arrives(url: string, to: Partial<Xyz>, timeout = 15_000): Promise<void> {
  await expect.poll(async () => near(await position(url), to), { timeout }).toBe(true);
}

/** The stage at `to`, homed first, on the fast clock. */
async function at(url: string, to: Xyz): Promise<void> {
  const on = new Api(url);
  await fast(url, async () => {
    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    await act("/api/v1/stage/move_to", { ...to, speed: 25_000 }, { on });
  });
}
const MIDDLE: Xyz = { x: 50_000, y: 37_500, z: 10_000 };

/** Control, once the pane has read where the stage is. */
async function control(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await expect(said(page, "position").locator("dd").first()).toHaveText(/^-?\d+\.\d µm$/);
}

/** Waits until the stage is at rest, and has said where since it came to rest. */
async function rested(url: string): Promise<Xyz> {
  await expect.poll(() => state(url)).toBe("idle");
  const then = (await sample(url)).time;
  await expect.poll(async () => (await sample(url)).time).toBeGreaterThan(then);
  return position(url);
}

test("after a start the stage waits to be homed, and Home homes it @main", async ({
  page,
  server,
}) => {
  await control(page, server.url);
  await expect(said(page, "alarm")).toContainText("The stage waits to be homed.");
  await expect(said(page, "stage-state")).toHaveText("In alarm");
  // Homing from the middle of the travel takes long at real time: the fast clock.
  await clockAt(server.url, FAST);
  try {
    await button(page, "Home").click();
    await expect(said(page, "stage-state")).toHaveText("Idle", { timeout: 30_000 });
  } finally {
    await clockAt(server.url, 1);
  }
  await expect(said(page, "alarm")).toHaveCount(0);
  // At the home corner: x and y at their least, z at its top.
  expect(near(await position(server.url), { x: 0, y: 0, z: 20_000 }, 5)).toBe(true);
});

test("a move to a place arrives, at real distances @main", async ({ page, server }) => {
  await homed(server.url);
  await control(page, server.url);
  // 20 mm along x and 15 along y, z left where it is: some 2.5 s at 10 mm/s.
  await field(page, "X").fill("20000");
  await field(page, "Y").fill("15000");
  await field(page, "Y").press("Enter");
  await button(page, "Move").click();
  await expect(said(page, "stage-state")).toHaveText("Moving");
  await arrives(server.url, { x: 20_000, y: 15_000, z: 20_000 }, 30_000);
  await expect(said(page, "stage-state")).toHaveText("Idle");
  const shown = said(page, "position").locator("dd").first();
  await expect
    .poll(async () => Math.abs(parseFloat(await shown.innerText()) - 20_000))
    .toBeLessThanOrEqual(3);
});

test("steps from the pad and from the keys on the image move by the step shown", async ({
  page,
  server,
}) => {
  await at(server.url, MIDDLE);
  await control(page, server.url);
  await step(page, "xy", "1 mm").click();
  await step(page, "z", "10 µm").click();
  let now = await rested(server.url);
  await button(page, "X towards its maximum").click();
  await arrives(server.url, { ...now, x: now.x + 1000 });
  now = await rested(server.url);
  await button(page, "Z towards its minimum").click();
  await arrives(server.url, { ...now, z: now.z - 10 });
  // The keys, with the image focused: the arrows along x and y, the page keys along z.
  now = await rested(server.url);
  await image(page).focus();
  await page.keyboard.press("ArrowUp");
  await arrives(server.url, { ...now, y: now.y - 1000 });
  now = await rested(server.url);
  await page.keyboard.press("PageUp");
  await arrives(server.url, { ...now, z: now.z + 10 });
});

test("a move past a limit is refused before it is sent", async ({ page, server }) => {
  await homed(server.url);
  await control(page, server.url);
  // At x's least: a step lower would pass it.
  await step(page, "xy", "100 µm").click();
  const sent = await invoked(server.url, "move_by");
  await button(page, "X towards its minimum").click();
  await expect(notices(page)).toContainText("X would pass its limits, 0.0 to 100000.0 µm");
  expect(await invoked(server.url, "move_by")).toBe(sent);
  // The other way, it is sent: homing left the stage a micrometre past its limits, which a
  // step along x that ends within them isn't refused for.
  const from = await rested(server.url);
  await button(page, "X towards its maximum").click();
  await arrives(server.url, { ...from, x: from.x + 100 });
  // Typed past the travel, the field itself refuses it.
  await field(page, "X").fill("150000");
  await field(page, "X").press("Enter");
  await expect(pane(page)).toContainText("At most 100000");
});

test("a jog moves while held, and stops when let go @main", async ({ page, server }) => {
  await at(server.url, MIDDLE);
  await control(page, server.url);
  await step(page, "xy", "While held").click();
  const from = await rested(server.url);
  // Held on the pad's button: it moves until it is let go.
  await button(page, "X towards its maximum").hover();
  await page.mouse.down();
  await expect.poll(async () => (await position(server.url)).x).toBeGreaterThan(from.x + 2000);
  await page.mouse.up();
  const stopped = await rested(server.url);
  expect(stopped.x).toBeLessThan(MIDDLE.x + 30_000);
  // And still there once the stage has said where it is again.
  expect(near(await rested(server.url), stopped, 0.5)).toBe(true);
  // Held on a key, with the image focused.
  await image(page).focus();
  await page.keyboard.down("ArrowLeft");
  await expect.poll(async () => (await position(server.url)).x).toBeLessThan(stopped.x - 2000);
  await page.keyboard.up("ArrowLeft");
  const again = await rested(server.url);
  expect(near(await rested(server.url), again, 0.5)).toBe(true);
});

test("a long move is halted by Escape, and by Halt", async ({ page, server }) => {
  await homed(server.url);
  await control(page, server.url);
  // 90 mm: nine seconds at 10 mm/s, stopped within the first.
  await field(page, "X").fill("90000");
  await field(page, "X").press("Enter");
  await button(page, "Move").click();
  await expect.poll(async () => (await position(server.url)).x).toBeGreaterThan(2000);
  await page.keyboard.press("Escape");
  const stopped = await rested(server.url);
  expect(stopped.x).toBeLessThan(30_000);
  // Again, halted by the button at the pane's foot.
  await button(page, "Move").click();
  await expect.poll(async () => (await position(server.url)).x).toBeGreaterThan(stopped.x + 1000);
  await page.getByRole("button", { name: "Halt", exact: true }).click();
  expect((await rested(server.url)).x).toBeLessThan(60_000);
});

test("an alarm is said with its reason, and cleared", async ({ page, server }) => {
  const on = new Api(server.url);
  await homed(server.url);
  await control(page, server.url);
  await act("/api/v1/simulation/inject_fault", { fault: "stage_alarm", active: true }, { on });
  await expect(said(page, "alarm")).toContainText("a limit switch (a fault injected)");
  await expect(said(page, "stage-state")).toHaveText("In alarm");
  await said(page, "alarm").getByRole("button", { name: "Clear the alarm" }).click();
  await expect(said(page, "stage-state")).toHaveText("Idle");
  await expect(said(page, "alarm")).toHaveCount(0);
});

test("without a calibration, pointing says what it lacks", async ({ page, server }) => {
  await control(page, server.url);
  await expect(said(page, "pointing")).toHaveText(
    "Pointing at the image needs the geometry calibration of the 10x at binning 1.",
  );
  // A double click says so too, and moves nothing.
  await image(page).dblclick();
  await expect(notices(page)).toContainText("needs the geometry calibration of the 10x");
  expect(await invoked(server.url, "move_by")).toBe(0);
});

/** The pixel of the active calibration's image, in µm, and its fit's error, in pixels. */
async function calibration(url: string): Promise<{ pixel: number; residual: number }> {
  const active = (await (await fetch(`${url}/api/v1/calibration/active`)).json()) as {
    geometry: { values: { pixel_um: number; residual_px: number } } | null;
  };
  return { pixel: active.geometry!.values.pixel_um, residual: active.geometry!.values.residual_px };
}

test("a double click centres the place within the calibration's error, and a drag pans", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  const on = new Api(server.url);
  await overTissue(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const ref = (await act("/api/v1/calibration/geometry", {}, { on })) as JobRef;
  await finished(ref, { on, timeout: 120_000 });
  const { pixel, residual } = await calibration(server.url);
  await control(page, server.url);
  await expect(said(page, "state").locator("dd")).toHaveText("Live", { timeout: 60_000 });
  const box = (await image(page).boundingBox())!;
  const middle: [number, number] = [box.x + box.width / 2, box.y + box.height / 2];
  // The pointer reads out the place on the stage, now that there is a calibration.
  await page.mouse.move(...middle);
  await expect(said(page, "sample")).toContainText("On the stage");
  await expect(said(page, "pointing")).toHaveText(/^Double-click the image/);

  // A CSS pixel's length at the sample: the frame, fitted, fills the canvas's width or height.
  const frame = { width: 960, height: 648 };
  const perCss = pixel / Math.min(box.width / frame.width, box.height / frame.height);
  // The error allowed, in CSS pixels: the fit's, the stage's backlash of 2 µm, and two pixels more.
  const allowed = residual * (pixel / perCss) + 2 / perCss + 2;
  const SIDE = 48;
  const REACH = 40;

  const before = await image(page).screenshot();
  const place: [number, number] = [middle[0] - box.width * 0.22, middle[1] + box.height * 0.18];
  const inShot = (point: [number, number]): [number, number] => [
    point[0] - box.x,
    point[1] - box.y,
  ];
  const wanted = await greys(page, before, inShot(place), SIDE);
  let sent = await invoked(server.url, "move_by");
  await page.mouse.dblclick(...place);
  await expect.poll(() => invoked(server.url, "move_by")).toBe(sent + 1);
  await rested(server.url);
  // Found again about the middle, within the error, once a frame from after the move is drawn.
  await expect
    .poll(
      async () => {
        const after = await image(page).screenshot();
        const best = found(
          wanted,
          await greys(page, after, inShot(middle), SIDE + 2 * REACH),
          SIDE,
          REACH,
        );
        return best.score > 0.6 && Math.hypot(best.dx, best.dy) <= allowed;
      },
      { timeout: 30_000 },
    )
    .toBe(true);

  // A drag, with the stage chosen: what is at the middle follows the pointer.
  await pane(page).getByRole("switch", { name: "A drag moves the stage" }).click();
  const centred = await greys(page, await image(page).screenshot(), inShot(middle), SIDE);
  const to: [number, number] = [middle[0] + 120, middle[1] - 60];
  await page.mouse.move(...middle);
  await page.mouse.down();
  await page.mouse.move(...to, { steps: 8 });
  sent = await invoked(server.url, "move_by");
  await page.mouse.up();
  await expect.poll(() => invoked(server.url, "move_by")).toBe(sent + 1);
  await rested(server.url);
  await expect
    .poll(
      async () => {
        const after = await image(page).screenshot();
        const best = found(
          centred,
          await greys(page, after, inShot(to), SIDE + 2 * REACH),
          SIDE,
          REACH,
        );
        return best.score > 0.6 && Math.hypot(best.dx, best.dy) <= allowed;
      },
      { timeout: 30_000 },
    )
    .toBe(true);
});

test("the stage's controls have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await control(page, server.url);
  await expect(said(page, "alarm")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the stage's controls look as their baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // Tall enough for the whole pane: scrolled, Halt would stand over it.
  await page.setViewportSize({ width: 1280, height: 3000 });
  await homed(server.url);
  await control(page, server.url);
  await expect(said(page, "stage-state")).toHaveText("Idle");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // Where homing left it is the simulator's to say, to a tenth of a µm: masked.
    await expect(pane(page)).toHaveScreenshot(`stage-${scheme}.png`, {
      mask: [said(page, "position").locator("dd"), pane(page).getByRole("spinbutton")],
    });
  }
  // The notes' pictures, at the usual size: the pane scrolled to the stage, Halt at its foot.
  await page.setViewportSize({ width: 1280, height: 800 });
  // The control pane scrolls once the window is shorter: then it is scrolled to the stage.
  const controls = page.getByRole("region", { name: "The controls" });
  await expect
    .poll(() => controls.evaluate((box) => box.scrollHeight > box.clientHeight))
    .toBe(true);
  // Scrolled at each look: the pane takes the window's height a moment after it, and a
  // scroll before then is undone (W3.7, whose panes made the pane overflow sooner).
  await expect
    .poll(async () => {
      await pane(page).evaluate((section) => section.scrollIntoView({ block: "start" }));
      return (await pane(page).boundingBox())!.y;
    })
    .toBeLessThan(100);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W3.3-stage-${scheme}`);
  }
});
