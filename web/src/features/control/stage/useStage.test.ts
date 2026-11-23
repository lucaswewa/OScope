// Moving the stage (W3.3): a step past the limits refused before it is sent,
// and a jog that always gets its halt, after its start, when its key is let
// go or the page loses the keys. On a stand-in network.
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createI18n } from "vue-i18n";

import { standInNetwork, standInSockets } from "../../../../tests/setup/stand-ins.ts";
import { toasts } from "@/ui/toasts";
import { useStage } from "./useStage";

/** The words it says, as the app gives them: a feature doesn't import the app's. */
const i18n = createI18n({
  legacy: false,
  locale: "en",
  messages: {
    en: {
      stage: {
        units: { um: "µm", mm: "mm" },
        beyond: "{axis} would pass its limits, {min} to {max} {unit}: the move wasn't sent.",
      },
    },
  },
});

const invocation = (action: string) => ({
  status: "completed",
  id: `${action}-1`,
  action,
  href: "",
  timeStarted: null,
  timeRequested: null,
  timeCompleted: null,
  links: [],
  input: {},
  output: null,
  log: [],
  error: null,
});

let made: Request[];
/** Where the stand-in stage says it is. */
let at: { x: number; y: number; z: number };
/** Lets the jog's start be answered: until then it is under way. */
let answerJog: () => void;

beforeEach(() => {
  setActivePinia(createPinia());
  standInSockets();
  at = { x: 99_950, y: 500, z: 10_000 };
  const travel = { min: 0, max: 100_000 };
  made = standInNetwork({
    "GET /api/v1/stage/capabilities": () => ({ x: travel, y: travel, z: { min: 0, max: 20_000 } }),
    "GET /api/v1/stage/position": () => ({ time: 0, position: at }),
    "POST /api/v1/stage/move_to": () =>
      new Response(JSON.stringify(invocation("move_to")), { status: 201 }),
    "GET /api/v1/stage/status": () => ({ state: "idle" }),
    "GET /api/v1/settings/display": () => ({ units: "um" }),
    "POST /api/v1/stage/move_by": () =>
      new Response(JSON.stringify(invocation("move_by")), { status: 201 }),
    "POST /api/v1/stage/halt": () =>
      new Response(JSON.stringify(invocation("halt")), { status: 201 }),
    "POST /api/v1/stage/jog": () =>
      new Promise<Response>((resolve) => {
        answerJog = () => resolve(new Response(JSON.stringify(invocation("jog")), { status: 201 }));
      }),
  });
});

/** The component the stage is used from: unmounted after each test, failed or not. */
let drawn: VueWrapper | undefined;

afterEach(async () => {
  drawn?.unmount();
  drawn = undefined;
  // A jog left by a failed test is halted as the component goes: on the stand-in network still.
  await flushPromises();
  vi.unstubAllGlobals();
  for (const one of toasts.shown.value) toasts.dismiss(one.id);
});

/** The stage, from a component, once its position and limits are read. */
async function aStage() {
  let stage!: ReturnType<typeof useStage>;
  const wrapper = mount(
    {
      setup() {
        stage = useStage();
        return () => null;
      },
    },
    { global: { plugins: [i18n] } },
  );
  drawn = wrapper;
  await vi.waitFor(() => {
    expect(stage.here.value && stage.capabilities.value.value).toBeTruthy();
  });
  return stage;
}

const posted = (path: string) =>
  made.filter((request) => request.method === "POST" && new URL(request.url).pathname === path);

/** Answers the `jogs`th jog's start, once it has been asked for. */
async function answered(jogs: number): Promise<void> {
  await vi.waitFor(() => expect(posted("/api/v1/stage/jog")).toHaveLength(jogs));
  answerJog();
}

describe("a step", () => {
  it("past the limits is refused before it is sent, and said", async () => {
    const stage = await aStage();
    await stage.step({ axis: "x", towardsMax: true }, 100);
    expect(posted("/api/v1/stage/move_by")).toEqual([]);
    expect(toasts.history.value[0]?.text).toBe(
      "X would pass its limits, 0.0 to 100000.0 µm: the move wasn't sent.",
    );
    // The other way, it is within them, and sent.
    await stage.step({ axis: "x", towardsMax: false }, 100);
    const [sent] = posted("/api/v1/stage/move_by");
    expect(await sent!.json()).toEqual({ dx: -100, dy: 0, dz: 0 });
  });

  it("from a micrometre past the limits, as homing leaves it, is sent where it goes", async () => {
    // Moved by from there, the server would find y and z past their limits.
    at = { x: -1, y: -1, z: 20_001 };
    const stage = await aStage();
    await stage.step({ axis: "x", towardsMax: true }, 100);
    expect(posted("/api/v1/stage/move_by")).toEqual([]);
    const [sent] = posted("/api/v1/stage/move_to");
    expect(await sent!.json()).toEqual({ x: 99, y: 0, z: 20_000 });
  });
});

describe("a jog", () => {
  it("is halted after its start is answered, however quickly it is let go", async () => {
    const stage = await aStage();
    stage.jog({ axis: "y", towardsMax: true });
    const ended = stage.endJog();
    await vi.waitFor(() => expect(posted("/api/v1/stage/jog")).toHaveLength(1));
    await flushPromises();
    // Halted now, the stage could stop before it started, then jog for ever.
    expect(posted("/api/v1/stage/halt")).toEqual([]);
    answerJog();
    await ended;
    const order = made
      .filter((one) => one.method === "POST")
      .map((one) => new URL(one.url).pathname);
    expect(order).toEqual(["/api/v1/stage/jog", "/api/v1/stage/halt"]);
  });

  it("is halted when the page loses the keys, and when it is left", async () => {
    const stage = await aStage();
    stage.jog({ axis: "x", towardsMax: false });
    await answered(1);
    globalThis.dispatchEvent(new Event("blur"));
    await vi.waitFor(() => expect(posted("/api/v1/stage/halt")).toHaveLength(1));
    // A second jog, and the component goes.
    stage.jog({ axis: "z", towardsMax: true });
    await answered(2);
    drawn?.unmount();
    drawn = undefined;
    await vi.waitFor(() => expect(posted("/api/v1/stage/halt")).toHaveLength(2));
    // Nothing jogs any more: losing the keys again halts nothing.
    globalThis.dispatchEvent(new Event("blur"));
    await flushPromises();
    expect(posted("/api/v1/stage/halt")).toHaveLength(2);
  });
});
