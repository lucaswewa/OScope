<script setup lang="ts">
import "./styles/toast.css";

import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import Button from "./Button.vue";
import Icon from "./Icon.vue";
import IconButton from "./IconButton.vue";
import { toasts as app, type Toast } from "./toasts";

/**
 * Draws the notices (ADR-0042) at the page's foot, the oldest first.
 *
 * - Each is read out as it shows: at once if it warns or says something
 *   failed, when the reader is free otherwise.
 * - Time stops for all of them while the pointer or the keyboard's focus is
 *   on one.
 * - F8 takes the focus to them. Escape there sends away the one with the
 *   focus, and gives the focus back when none is left.
 *
 * Not reka-ui's: every one of its notices listens for Escape anywhere, so
 * closing a menu would send away a failure that should stay until read.
 */
const props = withDefaults(defineProps<{ toasts?: typeof app }>(), { toasts: () => app });
const { t } = useI18n();
const list = ref<HTMLElement>();
const hovered = ref(false);
const focused = ref(false);
/** Where the focus was before F8 took it to the notices. */
let before: HTMLElement | null = null;

const urgent = (toast: Toast): boolean => toast.level === "warning" || toast.level === "danger";
const quiet = computed(() => props.toasts.shown.value.filter((toast) => !urgent(toast)));
const loud = computed(() => props.toasts.shown.value.filter(urgent));

watch([hovered, focused], ([pointer, keyboard]) => {
  if (pointer || keyboard) props.toasts.pause();
  else props.toasts.resume();
});

function hotkey(event: KeyboardEvent): void {
  if (event.key !== "F8" || props.toasts.shown.value.length === 0) return;
  event.preventDefault();
  if (!list.value?.contains(document.activeElement)) {
    before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }
  list.value?.focus();
}

onMounted(() => window.addEventListener("keydown", hotkey));
onBeforeUnmount(() => window.removeEventListener("keydown", hotkey));

/** Gives the focus back to where it was before F8, if that is still there. */
function back(): void {
  const to = before?.isConnected ? before : null;
  before = null;
  to?.focus();
}

/** Sends a notice away. One that held the focus passes it on: to the others, or back. */
function dismiss(toast: Toast): void {
  const item = list.value?.querySelector(`[data-toast="${toast.id}"]`);
  const held = item?.contains(document.activeElement) ?? false;
  props.toasts.dismiss(toast.id);
  if (!held) return;
  if (props.toasts.shown.value.length > 0) list.value?.focus();
  else back();
}

function act(toast: Toast): void {
  toast.action?.run();
  dismiss(toast);
}

function escape(event: KeyboardEvent): void {
  event.stopPropagation();
  const item = (event.target as Element).closest("[data-toast]");
  const toast = props.toasts.shown.value.find(
    (one) => String(one.id) === item?.getAttribute("data-toast"),
  );
  if (toast) dismiss(toast);
  else back();
}

function left(event: FocusEvent): void {
  if (!list.value?.contains(event.relatedTarget as Node | null)) focused.value = false;
}
</script>

<template>
  <section
    class="ui-toasts-region"
    :aria-label="t('ui.toasts.region', { hotkey: 'F8' })"
    @pointerenter="hovered = true"
    @pointerleave="hovered = false"
    @focusin="focused = true"
    @focusout="left"
  >
    <ol ref="list" class="ui-toasts" tabindex="-1" @keydown.escape="escape">
      <li
        v-for="toast in props.toasts.shown.value"
        :key="toast.id"
        class="ui-toast"
        :class="toast.level"
        :data-toast="toast.id"
      >
        <Icon :name="toast.level" small />
        <p class="ui-toast-text">{{ toast.text }}</p>
        <Button v-if="toast.action" variant="bare" compact @click="act(toast)">
          {{ toast.action.label }}
        </Button>
        <IconButton icon="close" :label="t('ui.dismiss')" compact @click="dismiss(toast)" />
      </li>
    </ol>
    <!-- What a screen reader is told, as each notice shows. These are there
         from the start: a region that appears with its words isn't read. -->
    <div class="ui-toasts-said" role="status">
      <p v-for="toast in quiet" :key="toast.id">{{ t("ui.toasts.notice") }}: {{ toast.text }}</p>
    </div>
    <div class="ui-toasts-said" role="alert">
      <p v-for="toast in loud" :key="toast.id">{{ t("ui.toasts.notice") }}: {{ toast.text }}</p>
    </div>
  </section>
</template>
