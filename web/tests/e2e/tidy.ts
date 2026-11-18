import { readdirSync, rmSync } from "node:fs";

import type { FullConfig, Reporter } from "@playwright/test/reporter";

/** Playwright's note of a run's result, which every run leaves. */
const NOTE = ".last-run.json";

/**
 * Removes a run's folder (`playwright.config.ts` gives each its own) when the
 * run leaves nothing in it but Playwright's note. Only failures stay in
 * `test-results/`, with their traces, until someone deletes them.
 */
export default class Tidy implements Reporter {
  private folder: string | undefined;

  onBegin(config: FullConfig): void {
    this.folder = config.projects[0]?.outputDir;
  }

  // Called after every reporter's `onEnd`, where Playwright writes its note.
  async onExit(): Promise<void> {
    if (this.folder === undefined) return;
    let left: string[];
    try {
      left = readdirSync(this.folder);
    } catch {
      return; // No folder: nothing ran.
    }
    if (left.every((name) => name === NOTE)) rmSync(this.folder, { recursive: true, force: true });
  }
}
