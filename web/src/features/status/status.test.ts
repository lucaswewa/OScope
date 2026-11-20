// The instrument's status (W2.2): the tone each state is shown in, the
// status kept by its property and its event, and the indicator. On a
// stand-in socket and network.
import { flushPromises, mount, RouterLinkStub, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";
import { createI18n } from "vue-i18n";

import {
  described,
  FakeSocket,
  standInNetwork,
  standInSockets,
} from "../../../tests/setup/stand-ins.ts";
import { Server } from "@/api/client";
import { useConnection } from "@/api/connection/connection";
import StatusIndicator from "./StatusIndicator.vue";
import { iconOf, noServer, toneOf, useInstrument, type Instrument } from "./status";

/** The status with these subsystems, and the state the server rolled them up to. */
function instrument(
  state: Instrument["state"],
  ...subsystems: [string, Instrument["state"]][]
): Instrument {
  return {
    state,
    message: state === "ok" ? "all well" : "stage: waiting to be homed",
    subsystems: subsystems.map(([subsystem, state]) => ({ subsystem, state, message: state })),
  };
}

/** The indicator's words, as the app gives them: a feature doesn't import the app's. */
const i18n = createI18n({
  legacy: false,
  locale: "en",
  messages: {
    en: {
      status: {
        title: "The instrument",
        label: "The instrument: {state}",
        unknown: "Unknown",
        untold: "The server hasn't told it.",
        health: "{state}: {message}",
        states: { ok: "OK", degraded: "Degraded", fault: "Fault", offline: "Offline" },
        subsystems: { camera: "The camera", stage: "The stage", lights: "The lights" },
        page: { link: "The status in full" },
      },
    },
  },
});

let read: Instrument;
let drawn: VueWrapper | undefined;

beforeEach(() => {
  setActivePinia(createPinia());
  read = instrument("fault", ["camera", "ok"], ["stage", "fault"]);
  standInSockets();
  standInNetwork({
    "GET /api/v1/thing_descriptions/": () => ({
      status: described({ instrument: {} }, ["changed"]),
    }),
    "GET /api/v1/status/instrument": () => read,
    "GET /health": () => ({ status: "ok" }),
  });
});

afterEach(() => {
  drawn?.unmount();
  drawn = undefined;
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

/** Waits for a condition, as an end-to-end test would. */
async function until(condition: () => boolean): Promise<void> {
  await vi.waitFor(() => expect(condition()).toBe(true), { timeout: 2000, interval: 5 });
}

/**
 * Tells the change the server would, on the status Thing's socket. Its
 * property says the same from then on: an event has the store read again
 * what it doesn't observe.
 */
function changed(now: Instrument, at: string): void {
  read = now;
  FakeSocket.last.says({ messageType: "event", data: { changed: { data: now, timestamp: at } } });
}

describe("a state's tone", () => {
  it("is its colour: ok well, degraded a warning, a fault a danger", () => {
    expect(toneOf("ok")).toBe("success");
    expect(toneOf("degraded")).toBe("warning");
    expect(toneOf("fault")).toBe("danger");
  });

  it("is neutral for what isn't there, or isn't known yet", () => {
    expect(toneOf("offline")).toBe("neutral");
    expect(toneOf(undefined)).toBe("neutral");
  });

  it("has an icon of its own, so the colour is never the only sign", () => {
    const icons = (["success", "warning", "danger", "neutral"] as const).map(iconOf);
    expect(new Set(icons).size).toBe(4);
  });
});

describe("the page without a server", () => {
  it("shows while the server is lost, refuses the app, or is of another version", () => {
    for (const state of ["reconnecting", "refused", "unknown version"] as const) {
      expect([state, noServer(state, false)]).toEqual([state, true]);
    }
    expect(noServer("connected", true)).toBe(false);
  });

  it("waits, at the start, until the server is late", () => {
    expect(noServer("connecting", false)).toBe(false);
    expect(noServer("connecting", true)).toBe(true);
  });
});

describe("the instrument's status", () => {
  it("is read at first, then each change the server tells", async () => {
    const on = new Server("http://127.0.0.1:8211");
    const scope = effectScope();
    const now = scope.run(() => useInstrument(on))!;
    await until(() => now.value?.state === "fault");

    FakeSocket.last.opens();
    const homed = instrument("ok", ["camera", "ok"], ["stage", "ok"]);
    changed(homed, "t1");
    await until(() => now.value?.state === "ok");
    expect(now.value).toEqual(homed);

    changed(instrument("fault", ["camera", "fault"], ["stage", "ok"]), "t2");
    await until(() => now.value?.state === "fault");
    expect(now.value?.subsystems[0]).toEqual({
      subsystem: "camera",
      state: "fault",
      message: "fault",
    });
    scope.stop();
  });
});

describe("the indicator", () => {
  it("takes the worst subsystem's colour, as the server weighs them, with each a click away", async () => {
    drawn = mount(StatusIndicator, {
      global: { plugins: [i18n], stubs: { RouterLink: RouterLinkStub } },
      attachTo: document.body,
    });
    const button = () => drawn!.get("[data-said='indicator']");
    // Without the server, the state isn't known.
    expect(button().text()).toBe("Unknown");
    await useConnection().look();
    await until(() => button().attributes("data-tone") === "danger");
    expect(button().text()).toBe("Fault");
    expect(button().attributes("aria-label")).toBe("The instrument: Fault");

    await button().trigger("click");
    await flushPromises();
    const panel = () => document.querySelector("[data-said='status']")!;
    expect(panel().textContent).toContain("stage: waiting to be homed");
    const rows = () =>
      [...panel().querySelectorAll("li")].map((li) => [li.dataset.tone, li.textContent?.trim()]);
    expect(rows()).toEqual([
      ["success", "The camera OK: ok"],
      ["danger", "The stage Fault: fault"],
    ]);

    // The stage homed, the lights dimmed: degraded, as the server rolls it up.
    FakeSocket.last.opens();
    changed(
      instrument("degraded", ["camera", "ok"], ["stage", "ok"], ["lights", "degraded"]),
      "t1",
    );
    await until(() => button().attributes("data-tone") === "warning");
    expect(rows().at(-1)).toEqual(["warning", "The lights Degraded: degraded"]);

    // A subsystem the app doesn't know goes by the server's name for it.
    changed(instrument("ok", ["spectrometer", "offline"]), "t2");
    await until(() => button().attributes("data-tone") === "success");
    expect(rows()).toEqual([["neutral", "spectrometer Offline: offline"]]);
  });
});
