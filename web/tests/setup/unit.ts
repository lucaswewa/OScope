// What every unit test of the app's code has around it (ADR-0040).
//
// - What a test mounted is unmounted after it: a component left behind keeps
//   its listeners on the window, and answers the next test's keys.
// - A test that makes Vue, or anything, warn or report an error on the
//   console fails: a warning nobody reads is a fault nobody finds. Vue says
//   there that a prop is wrong, that a listener put on a component went
//   nowhere, that something expected wasn't provided.
import { enableAutoUnmount } from "@vue/test-utils";
import { afterEach, beforeEach, expect, vi } from "vitest";

enableAutoUnmount(afterEach);

let said: string[] = [];

beforeEach(() => {
  said = [];
  for (const kind of ["warn", "error"] as const) {
    vi.spyOn(console, kind).mockImplementation((...parts: unknown[]) => {
      said.push(`console.${kind}: ${parts.map(String).join(" ").slice(0, 300)}`);
    });
  }
});

afterEach(() => {
  vi.restoreAllMocks();
  expect(said, "what the console was told during the test").toEqual([]);
});

/**
 * What the console has been told during this test, taken away: for a test
 * that means to make a component warn, and checks that it does.
 */
export function heard(): string[] {
  return said.splice(0);
}
