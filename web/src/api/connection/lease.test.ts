// The control lease (W1.3): what every control asks, and the warning before
// the lease lapses. On a stand-in socket and network, with the clock held
// by the test.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";

import {
  described,
  problem,
  standInNetwork,
  standInSockets,
} from "../../../tests/setup/stand-ins.ts";
import { activity } from "../client.ts";
import { useCanControl, useLease, WARN_BEFORE } from "./lease.ts";
import { useSession } from "./session.ts";

let control: boolean;

beforeEach(() => {
  setActivePinia(createPinia());
  control = false;
  activity.last = 0;
  standInSockets();
  standInNetwork({
    "GET /api/v1/thing_descriptions/": () => ({
      auth: described({ me: {}, lease: {}, accounts: {} }, ["lease_changed"]),
    }),
    "GET /api/v1/auth/me": () => ({ name: "alice", control }),
    "GET /api/v1/auth/lease": () => ({
      holder: control ? "alice" : "bob",
      since: null,
      requests: [],
      idle_s: 600,
    }),
    "GET /api/v1/auth/accounts": () => ["alice", "bob"],
  });
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "performance"],
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const later = (ms = 0) => vi.advanceTimersByTimeAsync(ms);

describe("whether this app may change the instrument", () => {
  it("is always so when the server has no accounts", async () => {
    standInNetwork({ "GET /api/v1/auth/me": () => problem(404, "Not Found", "no such Thing") });
    await useSession().probe();
    const scope = effectScope();
    const can = scope.run(() => useCanControl())!;
    expect(can.value).toBe(true);
    scope.stop();
  });

  it("is so with a session only while it holds the lease", async () => {
    await useSession().probe();
    const scope = effectScope();
    const can = scope.run(() => useCanControl())!;
    await later();
    expect(can.value).toBe(false);
    control = true;
    await later(2_000);
    expect(can.value).toBe(true);
    control = false;
    await later(2_000);
    expect(can.value).toBe(false);
    scope.stop();
  });

  it("isn't, while the app is logged out", async () => {
    standInNetwork({ "GET /api/v1/auth/me": () => problem(401, "Unauthorized", "a token") });
    await useSession().probe();
    const scope = effectScope();
    const can = scope.run(() => useCanControl())!;
    await later(2_000);
    expect(can.value).toBe(false);
    scope.stop();
  });
});

describe("the lease's holder", () => {
  it("is warned a minute before it lapses, and not after a change of its own", async () => {
    control = true;
    const scope = effectScope();
    const lease = scope.run(() => useLease())!;
    await later(1_000);
    expect(lease.holding.value).toBe(true);
    expect(lease.lapsing.value).toBe(false);
    await later(600_000 - WARN_BEFORE);
    expect(lease.lapsing.value).toBe(true);
    expect(lease.lapsesIn.value).toBeLessThanOrEqual(WARN_BEFORE);
    // A change keeps it.
    activity.last = performance.now();
    await later(1_000);
    expect(lease.lapsing.value).toBe(false);
    scope.stop();
  });

  it("isn't warned of a lease it doesn't hold", async () => {
    const scope = effectScope();
    const lease = scope.run(() => useLease())!;
    await later(600_000);
    expect([lease.holding.value, lease.lapsesIn.value, lease.lapsing.value]).toEqual([
      false,
      undefined,
      false,
    ]);
    scope.stop();
  });
});
