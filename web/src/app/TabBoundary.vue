<script setup lang="ts">
import { onErrorCaptured, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import Button from "@/ui/Button.vue";

/**
 * A tab's own failures (W2.1): an error thrown inside it, or its page that
 * couldn't load, stops that tab and not the app. It says what went wrong,
 * and offers to load the tab again: drawn afresh, or, for a page that
 * couldn't load, its navigation tried again. Another tab starts afresh.
 */
const props = defineProps<{
  /** The tab shown: a change of it clears a failure. */
  tab: string;
}>();

const { t } = useI18n();
const failure = shallowRef<Error>();
/** What tries again a page that couldn't load. */
let again: (() => void) | undefined;

onErrorCaptured((error) => {
  fail(error);
  return false;
});
watch(
  () => props.tab,
  () => (failure.value = undefined),
);

/**
 * Stops the tab with `error`: the shell's, for a page that couldn't load,
 * with `retry` to try its navigation again.
 */
function fail(error: unknown, retry?: () => void): void {
  failure.value = error instanceof Error ? error : new Error(String(error));
  again = retry;
}

/** The tab drawn afresh: the failure gone, the tab is mounted anew. */
function reload(): void {
  failure.value = undefined;
  again?.();
  again = undefined;
}

/** Tries again a page that couldn't load, if one couldn't: once the server is back (W2.2). */
function retry(): void {
  if (failure.value && again) reload();
}

defineExpose({ fail, retry });
</script>

<template>
  <section v-if="failure" class="failed" role="alert" data-said="failed">
    <h1 tabindex="-1">{{ t("shell.failed.title") }}</h1>
    <p>{{ t("shell.failed.said", { message: failure.message }) }}</p>
    <Button variant="primary" icon="refresh" @click="reload">{{ t("shell.failed.reload") }}</Button>
  </section>
  <div v-else class="tab">
    <slot />
  </div>
</template>

<style scoped>
.failed {
  display: grid;
  gap: var(--space-3);
  justify-items: start;
  padding: var(--space-6);
}

.failed h1 {
  margin: 0;
}

.failed p {
  margin: 0;
  color: var(--danger);
}

.tab {
  display: contents;
}
</style>
