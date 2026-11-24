// The steps the checkpoints share, taken in the app as a person would: the
// stage homed and moved over the tissue on the simulator's fast clock, at
// their real distances, and the camera streaming.
import type { Page } from "@playwright/test";

import { Server as Api, call } from "../../src/api/client.ts";
import { expect } from "../e2e/fixtures.ts";
import { clockAt, FAST, SMALL, TISSUE } from "../e2e/sim.ts";

export const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
export const readout = (page: Page, name: string) => said(page, name).locator("dd");
export const stage = (page: Page) => said(page, "stage");
export const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });

/** A step to choose, `xy` or `z`: a button that stays pressed. */
export const step = (page: Page, axes: "xy" | "z", name: string) =>
  stage(page)
    .getByRole("group", { name: axes === "xy" ? "Step in X and Y" : "Step in Z" })
    .getByRole("button", { name, exact: true });

/** The stage's place, as the server has it. */
export async function at(url: string): Promise<{ x: number; y: number; z: number }> {
  const on = new Api(url);
  return (await call((signal) => on.api.GET("/api/v1/stage/position", { signal }))).position;
}

/** Waits until the stage is within 3 µm of `to`, along the axes given, and still. */
export async function arrives(
  page: Page,
  url: string,
  to: Partial<{ x: number; y: number; z: number }>,
  timeout = 60_000,
): Promise<void> {
  const off = async () => {
    const now = await at(url);
    return Math.max(
      ...(["x", "y", "z"] as const).map((axis) =>
        to[axis] === undefined ? 0 : Math.abs(now[axis] - to[axis]),
      ),
    );
  };
  await expect.poll(off, { timeout }).toBeLessThanOrEqual(3);
  await expect(said(page, "stage-state")).toHaveText("Idle", { timeout });
}

/**
 * On Control: the stage homed, then moved to `to` by its fields, on the fast
 * clock, the moves their real lengths.
 */
export async function homeAndMove(
  page: Page,
  url: string,
  to: { x: number; y: number; z: number } = { ...TISSUE, z: 5000 },
): Promise<void> {
  await expect(said(page, "alarm")).toContainText("The stage waits to be homed.");
  await clockAt(url, FAST);
  try {
    await stage(page).getByRole("button", { name: "Home", exact: true }).click();
    await expect(said(page, "stage-state")).toHaveText("Idle", { timeout: 60_000 });
    await moveTo(page, url, to);
  } finally {
    await clockAt(url, 1);
  }
}

/** The stage moved to `to` by its fields and *Move*. */
export async function moveTo(
  page: Page,
  url: string,
  to: { x: number; y: number; z: number },
): Promise<void> {
  const field = (axis: string) =>
    stage(page).getByRole("spinbutton", { name: new RegExp(`^${axis}`) });
  await field("X").fill(String(to.x));
  await field("Y").fill(String(to.y));
  await field("Z").fill(String(to.z));
  await field("Z").press("Enter");
  await stage(page).getByRole("button", { name: "Move", exact: true }).click();
  await arrives(page, url, to);
}

/**
 * The camera streaming through `region` of its sensor, by *Start*. The region
 * is the server's: a debug server renders each frame, so its tests use a part.
 */
export async function stream(page: Page, url: string, region = SMALL): Promise<void> {
  const on = new Api(url);
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: region, signal }));
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
}

/** A project made from a pane's project fields, the capture pane's or the scan's: chosen there. */
export async function makeProject(page: Page, name: string, pane = "capture"): Promise<void> {
  const fields = said(page, pane);
  await fields.getByRole("textbox", { name: "New project" }).fill(name);
  await fields.getByRole("button", { name: "Make", exact: true }).click();
  await expect(fields.getByRole("combobox", { name: "Project" })).toContainText(name);
}
