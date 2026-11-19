<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import { useConnection } from "@/api/connection/connection";
import { useCanControl } from "@/api/connection/lease";
import { onActionFailed } from "@/api/things/use";
import ReadOnlyLine from "@/app/ReadOnlyLine.vue";
import ShortcutsDialog from "@/app/ShortcutsDialog.vue";
import TabBoundary from "@/app/TabBoundary.vue";
import { addShortcut, listen } from "@/app/shortcuts";
import { TABS, tabOf } from "@/app/tabs";
import ConnectionLost from "@/features/status/ConnectionLost.vue";
import RailStatus from "@/features/status/RailStatus.vue";
import { useLeaseRequests } from "@/features/status/leaseRequests";
import { noServer } from "@/features/status/status";
import { host } from "@/host/host";
import ControlScope from "@/ui/ControlScope.vue";
import Rail from "@/ui/Rail.vue";
import type { RailItem } from "@/ui/rail";
import { notify } from "@/ui/toasts";

/**
 * The app's frame (W2.1, ADR-0048): the rail, and the tab it shows. The tab
 * is locked while another holds the control lease, catches its own
 * failures, names the window, and takes the focus to its heading when the
 * route changes. The keys 1 to 9 open the tabs, and `?` lists the keys.
 * While there is no server, a page says so in place of the tab, which is
 * kept as it was (W2.2); at the start, only once the server is late.
 */
const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const canControl = useCanControl();
const listing = ref(false);
const content = ref<HTMLElement>();
const boundary = ref<InstanceType<typeof TabBoundary>>();
const connection = useConnection();
useLeaseRequests();

/** How long the server may take to answer at the start before the app says it is looking. */
const LATE = 2_000;
const late = shallowRef(false);
const lateTimer = setTimeout(() => (late.value = true), LATE);
const lost = computed(() => noServer(connection.state, late.value));
// A page that couldn't load while the server was gone is tried again once it is back.
watch(lost, (now, before) => {
  if (before && !now) boundary.value?.retry();
});

function items(bottom: boolean): RailItem[] {
  return TABS.filter((tab) => Boolean(tab.bottom) === bottom).map((tab) => ({
    href: router.resolve({ name: tab.name }).href,
    label: t(`tabs.${tab.name}.short`),
    icon: tab.icon,
    current: route.name === tab.name,
  }));
}
const top = computed(() => items(false));
const bottom = computed(() => items(true));

/** The window named for the tab shown. */
function name(routeName: unknown): void {
  const tab = tabOf(routeName);
  host().window.title(tab ? t("shell.title", { tab: t(`tabs.${tab.name}.title`) }) : t("app.name"));
}
name(route.name);

const stops = [
  ...TABS.map((tab, at) =>
    addShortcut({
      key: String(at + 1),
      label: () => t("shell.keys.tab", { tab: t(`tabs.${tab.name}.title`) }),
      run: () => void router.push({ name: tab.name }),
    }),
  ),
  addShortcut({ key: "?", label: () => t("shell.keys.list"), run: () => (listing.value = true) }),
  listen(),
  // An action the server refused, in the server's words.
  onActionFailed((error) =>
    notify({ level: "danger", text: t("shell.refused", { why: error.message }) }),
  ),
  router.afterEach((to, from) => {
    name(to.name);
    if (to.name === from.name) return;
    // The new tab's heading takes the focus, as a screen reader's user expects.
    void nextTick(() => {
      const heading = content.value?.querySelector<HTMLElement>("h1");
      heading?.setAttribute("tabindex", "-1");
      heading?.focus();
    });
  }),
  // A page that couldn't load stops its tab, not the app. Tried again and
  // failing again, it is told here once more: its promise has nothing to add.
  router.onError((error, to) =>
    boundary.value?.fail(error, () => void router.push(to.fullPath).catch(() => undefined)),
  ),
];
onBeforeUnmount(() => {
  stops.forEach((stop) => stop());
  clearTimeout(lateTimer);
});

function toContent(): void {
  content.value?.focus();
}
</script>

<template>
  <div class="shell">
    <button class="skip" type="button" @click="toContent">{{ t("shell.skip") }}</button>
    <Rail :label="t('shell.rail')" :top="top" :bottom="bottom">
      <template #end>
        <RailStatus />
      </template>
    </Rail>
    <main id="content" ref="content" class="content" tabindex="-1">
      <ConnectionLost v-if="lost" />
      <div v-show="!lost" class="tab">
        <ReadOnlyLine />
        <TabBoundary ref="boundary" :tab="String(route.name)">
          <ControlScope :locked="!canControl">
            <RouterView />
          </ControlScope>
        </TabBoundary>
      </div>
    </main>
    <ShortcutsDialog v-model:open="listing" />
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  block-size: 100vh;
  background: var(--surface-page);
}

.content {
  display: flex;
  flex-direction: column;
  min-block-size: 0;
  overflow: auto;
}

.content:focus {
  outline: none;
}

.tab {
  display: contents;
}

.skip {
  position: fixed;
  z-index: 10;
  inset-block-start: var(--space-2);
  inset-inline-start: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--accent);
  background: var(--surface-raised);
  color: var(--accent-text);
  font: inherit;
  transform: translateY(-200%);
}

.skip:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
  transform: none;
}
</style>
