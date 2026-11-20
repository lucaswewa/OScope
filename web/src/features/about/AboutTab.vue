<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { fetched, server } from "@/api/client";
import { appNotices, type AppNotice } from "@/api/notices";
import { serverVersion } from "@/api/system";
import { useProperty } from "@/api/things/use";
import { upFor } from "@/features/about/about";
import { useServerVersion } from "@/features/about/serverVersion";
import { sizeOf } from "@/features/logging/logs";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Collapsible from "@/ui/Collapsible.vue";
import Dialog from "@/ui/Dialog.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * About (W4.3): the server's version and commit, the computer it runs on,
 * how long it has been up; and the licences, the server's notices from its
 * package and the app's own, made from its packages at build.
 */
const { t } = useI18n();
const version = useServerVersion(serverVersion);
const commit = useProperty("/api/v1/system/git_commit");
const os = useProperty("/api/v1/system/os");
const cpu = useProperty("/api/v1/system/cpu");
const memory = useProperty("/api/v1/system/memory", { every: 10_000 });
const uptime = useProperty("/api/v1/system/uptime", { every: 10_000 });
const notices = useProperty("/api/v1/system/notices");

const size = (bytes: number) => {
  const { value, unit } = sizeOf(bytes);
  return `${value} ${unit}`;
};
const entries = computed(() => {
  const said: { term: string; value: string }[] = [];
  const add = (term: string, value: string | undefined) => {
    if (value !== undefined) said.push({ term: t(`about.system.${term}`), value });
  };
  add("commit", commit.value.value);
  const system = os.value.value;
  add("os", system && t("about.system.osIs", { ...system }));
  const processor = cpu.value.value;
  add("cpu", processor && t("about.system.cpuIs", { ...processor }));
  const held = memory.value.value;
  const free = held && size(held.available_bytes);
  add("memory", held && t("about.system.memoryIs", { total: size(held.total_bytes), free }));
  const up = uptime.value.value;
  add("uptime", up === undefined ? undefined : upFor(up));
  return said;
});

// The app's own, read once.
const own = shallowRef<AppNotice[]>();
const ownFailed = shallowRef(false);
onMounted(async () => {
  try {
    own.value = await appNotices();
  } catch {
    ownFailed.value = true;
  }
});

// A notice of the server's, read in a dialog: HTML in a frame that runs nothing.
const reading = shallowRef<{ name: string; text: string; html: boolean }>();
const open = computed({
  get: () => reading.value !== undefined,
  set: (now: boolean) => {
    if (!now) reading.value = undefined;
  },
});
const fetching = shallowRef<string>();
async function read(name: string): Promise<void> {
  fetching.value = name;
  try {
    const path = `/api/v1/system/notices/${encodeURIComponent(name)}`;
    const text = await fetched(server.address(path), (answer) => answer.text());
    reading.value = { name, text, html: name.endsWith(".html") };
  } catch (thrown) {
    reading.value = { name, text: String(thrown), html: false };
  } finally {
    fetching.value = undefined;
  }
}

/**
 * The notice's frame as tall as its document: the box around it scrolls,
 * and takes the keys. Focused, the frame would keep them from the page, as
 * WebKit's sandboxed frames do even from its listeners, and Escape wouldn't
 * close the dialog.
 */
function fitted(event: Event): void {
  const frame = event.target as HTMLIFrameElement;
  const height = frame.contentDocument?.documentElement.scrollHeight;
  if (height) frame.style.blockSize = `${height}px`;
}
</script>

<template>
  <section class="about">
    <h1>{{ t("tabs.about.title") }}</h1>
    <p role="status" :data-state="version.kind">
      <template v-if="version.kind === 'connected'">
        {{ t("home.version", { version: version.version }) }}
      </template>
      <template v-else-if="version.kind === 'unreachable'">
        {{ t("home.unreachable") }}
      </template>
      <template v-else>{{ t("home.connecting") }}</template>
    </p>
    <Card :title="t('about.system.title')" :level="2" data-said="system">
      <KeyValue :entries="entries" />
    </Card>
    <Card :title="t('about.licences.title')" :level="2" data-said="licences">
      <h3>{{ t("about.licences.server") }}</h3>
      <ul v-if="notices.value.value?.length" class="files" data-said="server-notices">
        <li v-for="notice in notices.value.value" :key="notice.name">
          <span class="name">{{ notice.name }}</span>
          <Button
            compact
            variant="bare"
            :busy="fetching === notice.name"
            :aria-label="t('about.licences.readNamed', { name: notice.name })"
            @click="read(notice.name)"
          >
            {{ t("about.licences.read") }}
          </Button>
        </li>
      </ul>
      <p v-else class="said">{{ t("about.licences.serverNone") }}</p>
      <h3>{{ t("about.licences.app") }}</h3>
      <p v-if="ownFailed" class="said">{{ t("about.licences.appNone") }}</p>
      <template v-else-if="own">
        <p class="said" data-said="app-count">
          {{ t("about.licences.appCount", { count: own.length }, own.length) }}
        </p>
        <ul class="packages" data-said="app-notices">
          <li v-for="notice in own" :key="notice.name">
            <Collapsible :title="`${notice.name} ${notice.version} · ${notice.licence}`" :level="4">
              <pre v-if="notice.text">{{ notice.text }}</pre>
              <p v-else class="said">
                {{ t("about.licences.noText", { licence: notice.licence }) }}
              </p>
            </Collapsible>
          </li>
        </ul>
      </template>
    </Card>
    <Dialog v-model:open="open" :title="reading?.name ?? ''">
      <div
        v-if="reading?.html"
        class="notice"
        role="region"
        :aria-label="reading.name"
        tabindex="0"
      >
        <iframe
          sandbox="allow-same-origin"
          tabindex="-1"
          :srcdoc="reading.text"
          :title="reading.name"
          @load="fitted"
        />
      </div>
      <pre
        v-else-if="reading"
        class="notice"
        role="region"
        :aria-label="reading.name"
        tabindex="0"
        >{{ reading.text }}</pre>
    </Dialog>
  </section>
</template>

<style scoped>
.about {
  display: grid;
  gap: var(--space-4);
  align-content: start;
  max-inline-size: 60rem;
  padding: var(--space-6);
}

.about h1,
.about h3,
.about p,
.about ul {
  margin: 0;
}

.about p[data-state="unreachable"] {
  color: var(--danger);
}

.said {
  color: var(--text-muted);
}

.files,
.packages {
  display: grid;
  gap: var(--space-1);
  padding: 0;
  list-style: none;
}

.files li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.packages {
  max-block-size: 24rem;
  overflow: auto;
}

pre {
  margin: 0;
  font-size: var(--font-size-small);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.notice {
  inline-size: min(48rem, 80vw);
  block-size: 60vh;
  border: 1px solid var(--border);
  background: var(--surface-raised);
  overflow: auto;
}

.notice iframe {
  display: block;
  inline-size: 100%;
  border: 0;
}
</style>
