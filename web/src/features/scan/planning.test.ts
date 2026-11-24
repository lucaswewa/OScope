// The scan tab's planning (W5.1), which its panes share: it asks for the plan for as
// long as the page lasts, not only as long as the tab that first made it, and only
// when what it asks changes (W10.1).
import { effectScope } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/api/actions", () => ({ act: vi.fn(async () => ({ plan: { tiles: [] } })) }));

afterEach(() => {
  vi.useRealTimers();
  vi.resetModules();
});

/** Made first in a tab's scope, which ends as another tab opens. */
async function afterTheTabWent<T>(use: () => T): Promise<T> {
  const tab = effectScope();
  tab.run(use);
  tab.stop();
  return use();
}

describe("the scan tab's planning", () => {
  it("asks for the plan still, once the tab that made it has gone", async () => {
    vi.useFakeTimers();
    const { act } = await import("@/api/actions");
    const { usePlanning } = await import("@/features/scan/planning");
    const plan = await afterTheTabWent(usePlanning);
    plan.region.value = { x: 0, y: 0, width: 1000, height: 1000 };
    await vi.advanceTimersByTimeAsync(400);
    expect(act).toHaveBeenCalledWith("/api/v1/scan/plan", expect.anything(), expect.anything());
  });

  it("asks again only when what it asks changes, not for an equal request made anew", async () => {
    vi.useFakeTimers();
    const { act } = await import("@/api/actions");
    const { usePlanning } = await import("@/features/scan/planning");
    const plan = usePlanning();
    plan.region.value = { x: 0, y: 0, width: 1000, height: 1000 };
    await vi.advanceTimersByTimeAsync(400);
    plan.region.value = { x: 0, y: 0, width: 1000, height: 1000 };
    await vi.advanceTimersByTimeAsync(400);
    expect(act).toHaveBeenCalledTimes(1);
    plan.region.value = { x: 0, y: 0, width: 2000, height: 1000 };
    await vi.advanceTimersByTimeAsync(400);
    expect(act).toHaveBeenCalledTimes(2);
  });
});
