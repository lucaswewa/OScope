// Power (W4.3): an action goes on only once its name is typed, then is asked
// of the server confirmed; a dry run says what it would have run; refused
// while a hardware job runs, it is forced, its name typed again. On a
// stand-in socket and network.
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { h } from "vue";
import { createI18n } from "vue-i18n";

import { described, standInNetwork, standInSockets } from "../../../tests/setup/stand-ins.ts";
import Provider from "@/ui/Provider.vue";
import ui from "@/ui/words/en";
import { isBack } from "./power";
import PowerTab from "./PowerTab.vue";

/** The words the test looks for: the rest are left out, unwarned. */
const i18n = () =>
  createI18n({
    legacy: false,
    locale: "en",
    missingWarn: false,
    fallbackWarn: false,
    messages: {
      en: {
        ui,
        power: {
          force: "Force it…",
          reboot: { button: "Reboot…", confirm: "Reboot", deed: "rebooting" },
          shutdown: { button: "Shut down…", confirm: "Shut down", deed: "shutting down" },
          said: { dry: "{action} would have run {command}" },
        },
      },
    },
  });

/** An invocation of a power action, as the server answers it. */
const invocation = (status: string, output: unknown = null, error: unknown = null) => ({
  status,
  id: "an-invocation",
  action: "",
  href: "",
  timeStarted: null,
  timeRequested: null,
  timeCompleted: null,
  links: [],
  input: {},
  output,
  log: [],
  error,
});

/** What each action asked of the server, and how its invocations end, in turn. */
let asked: { action: string; body: unknown }[];
let ends: ReturnType<typeof invocation>[];
let drawn: VueWrapper | undefined;

beforeEach(() => {
  setActivePinia(createPinia());
  asked = [];
  ends = [];
  standInSockets();
  const start = (action: string) => async (request: Request) => {
    asked.push({ action, body: await request.json() });
    return new Response(JSON.stringify(invocation("pending")), { status: 201 });
  };
  standInNetwork({
    "GET /api/v1/thing_descriptions/": () => ({
      power: described({ dry_run: {}, dry_run_restart: {} }, ["powering"]),
    }),
    "GET /api/v1/power/dry_run": () => true,
    "GET /api/v1/power/dry_run_restart": () => true,
    "POST /api/v1/power/reboot": start("reboot"),
    "POST /api/v1/power/shutdown": start("shutdown"),
    "GET /api/v1/action_invocations/an-invocation": () => ends.shift(),
  });
  drawn = mount(
    { render: () => h(Provider, () => h(PowerTab)) },
    { attachTo: document.body, global: { plugins: [i18n()] } },
  );
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

const buttons = () => [...document.querySelectorAll<HTMLButtonElement>("button")];
const button = (text: string) => buttons().find((one) => one.textContent?.trim() === text);
const dialog = () => document.querySelector("[role='alertdialog']");
const said = () => document.querySelector("[data-said='power'] p")?.textContent?.trim();

/** Opens `action`'s confirmation from its card's button, labelled `opens`. */
async function open(opens: string): Promise<void> {
  await until(() => button(opens) !== undefined);
  button(opens)!.click();
  await until(() => dialog() !== null);
}

/** Types `word` in the open confirmation's field. */
async function type(word: string): Promise<void> {
  const field = dialog()!.querySelector("input")!;
  field.value = word;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await flushPromises();
}

const powered = (action: string, command: string[]) =>
  invocation("completed", { action, command, dry_run: true });

describe("a power action", () => {
  it("goes on only once its name is typed, and says what a dry run would have run", async () => {
    ends = [powered("reboot", ["shutdown.exe", "/r", "/t", "0"])];
    await open("Reboot…");
    const go = () => button("Reboot")!;
    expect(go().disabled).toBe(true);
    await type("rebot");
    expect(go().disabled).toBe(true);
    await type("reboot");
    expect(go().disabled).toBe(false);
    go().click();
    await until(() => said() === "rebooting would have run shutdown.exe /r /t 0");
    expect(asked).toEqual([{ action: "reboot", body: { confirm: true, force: false } }]);
  });

  it("is refused while a hardware job runs, and forced, its name typed again", async () => {
    const why =
      "conflict (409): capture.burst is running on the instrument: let it end, cancel it, or force it";
    ends = [
      invocation("error", null, { detail: why }),
      powered("shutdown", ["systemctl", "poweroff"]),
    ];
    await open("Shut down…");
    await type("shutdown");
    button("Shut down")!.click();
    await until(() => said()?.startsWith("capture.burst is running") === true);
    expect(asked).toEqual([{ action: "shutdown", body: { confirm: true, force: false } }]);
    // Forced: asked again, its name typed again.
    await until(() => dialog() === null);
    await open("Force it…");
    expect(button("Shut down")!.disabled).toBe(true);
    await type("shutdown");
    button("Shut down")!.click();
    await until(() => said() === "shutting down would have run systemctl poweroff");
    expect(asked[1]).toEqual({ action: "shutdown", body: { confirm: true, force: true } });
  });
});

describe("the server after a restart", () => {
  it("is back once it has been up for less time than since the restart was asked", () => {
    // Asked 4 s ago: up for 5 s, it is the one asked, still stopping; for 2 s, a new one.
    expect(isBack(5, 4_000)).toBe(false);
    expect(isBack(2, 4_000)).toBe(true);
  });
});
