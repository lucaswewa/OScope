// For the components' own tests: a component mounted as the app has it, with
// the components' words, inside their provider, in a page.
import { mount, type VueWrapper } from "@vue/test-utils";
import { h, type Component } from "vue";
import { createI18n } from "vue-i18n";

import Provider from "./Provider.vue";
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
