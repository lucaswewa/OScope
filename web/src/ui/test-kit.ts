// For the components' own tests: a component mounted as the app has it, with
// the components' words, inside their provider, in a page.
import { mount, type VueWrapper } from "@vue/test-utils";
import { h, type Component } from "vue";
import { createI18n } from "vue-i18n";

import Provider from "./Provider.vue";
import type { Clock } from "./toasts";
import ui from "./words/en";

type Props = Record<string, unknown>;
type Slots = Record<string, () => unknown>;

/**
 * Mounts `component` in the page, and gives the wrapper of what it drew.
 * `props` may change later through `set`; `v-model` is `modelValue` and
 * `onUpdate:modelValue`, as in any render function.
 */
export function draw(component: Component, props: Props = {}, slots: Slots = {}) {
  const state = { props };
  const wrapper = mount(
    { render: () => h(Provider, () => h(component, state.props, slots)) },
    {
      attachTo: document.body,
      global: {
        plugins: [createI18n({ legacy: false, locale: "en", messages: { en: { ui } } })],
      },
    },
  );
  return {
    wrapper: wrapper as VueWrapper,
    /** Changes its props, as a parent would. */
    async set(changed: Props): Promise<void> {
      state.props = { ...state.props, ...changed };
      wrapper.vm.$forceUpdate();
      await wrapper.vm.$nextTick();
    },
  };
}

/**
 * A component with a `v-model`, held as a parent holds it: what it emits
 * comes back as its value. `changes` lists what it emitted.
 */
export function drawModel<T>(component: Component, value: T, props: Props = {}, slots: Slots = {}) {
  const changes: T[] = [];
  const held = { value };
  const drawn = draw(
    component,
    {
      ...props,
      modelValue: value,
      "onUpdate:modelValue": (now: T) => {
        changes.push(now);
        held.value = now;
        void drawn.set({ modelValue: now });
      },
    },
    slots,
  );
  return { ...drawn, changes, held };
}

/** A clock a test turns by hand: its timers run only when it is moved on. */
export function handClock() {
  let now = 0;
  let next = 1;
  const timers = new Map<number, { at: number; run: () => void }>();
  const clock: Clock = {
    set(run, ms) {
      const id = next++;
      timers.set(id, { at: now + ms, run });
      return id;
    },
    clear: (timer) => void timers.delete(timer as number),
    now: () => now,
  };
  return {
    clock,
    /** Moves time on by `ms`, running each timer that falls due, in turn. */
    pass(ms: number): void {
      const until = now + ms;
      for (;;) {
        const due = [...timers].filter(([, timer]) => timer.at <= until);
        if (due.length === 0) break;
        const [id, timer] = due.sort((a, b) => a[1].at - b[1].at)[0]!;
        timers.delete(id);
        now = timer.at;
        timer.run();
      }
      now = until;
    },
    /** How many timers wait to run. */
    running: () => timers.size,
  };
}
