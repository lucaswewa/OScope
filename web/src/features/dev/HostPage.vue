<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import { host } from "@/host/host";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Field from "@/ui/Field.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Switch from "@/ui/Switch.vue";
import TextArea from "@/ui/TextArea.vue";
import TextField from "@/ui/TextField.vue";
import { notify } from "@/ui/toasts";

/**
 * The host adapter (W1.4): each of the platform's services, tried by hand.
 * In a browser: a download, the file chooser, the clipboard, a
 * notification, the document's title and full screen, and a new tab.
 */
const { t } = useI18n();
const platform = host();

const text = ref(t("host.files.sample"));
const name = ref("oscope-note.txt");
const opened = ref<File>();
const begins = ref("");
const told = ref("");
const title = ref(t("host.window.sample"));
const full = ref(platform.window.isFullScreen());
// Relative, the gallery's own: a web page's address may be typed in its place.
const address = ref("#/dev/gallery");
const remembered = kept(
  "oscope.gallery.host",
  "",
  (value): value is string => typeof value === "string",
);

const reason = (error: unknown): string => (error instanceof Error ? error.message : String(error));

watch(title, (now) => platform.window.title(now));
const stop = platform.window.onFullScreen((now) => (full.value = now));
onBeforeUnmount(() => {
  stop();
  platform.window.title(t("app.name"));
});

async function save(): Promise<void> {
  const file = name.value.trim() || "oscope-note.txt";
  await platform.files.save(file, new Blob([text.value], { type: "text/plain" }));
  notify({ level: "success", text: t("host.files.saved", { name: file }) });
}

async function open(): Promise<void> {
  const [file] = await platform.files.open({ accept: [".txt", ".csv", ".json", "text/*"] });
  if (!file) return;
  begins.value = (await file.slice(0, 200).text()).split("\n")[0] ?? "";
  opened.value = file;
}

async function copy(): Promise<void> {
  try {
    await platform.copy(text.value);
    told.value = t("host.tell.copied");
  } catch (error) {
    told.value = t("host.tell.notCopied", { reason: reason(error) });
  }
}

async function raise(): Promise<void> {
  const shown = await platform.notify(t("app.name"), text.value.split("\n")[0]);
  told.value = shown ? t("host.tell.shown") : t("host.tell.notShown");
}

async function fullScreen(on: boolean): Promise<void> {
  try {
    await (on ? platform.window.enterFullScreen() : platform.window.leaveFullScreen());
  } catch (error) {
    full.value = platform.window.isFullScreen();
    notify({ level: "warning", text: t("host.window.notFull", { reason: reason(error) }) });
  }
}

function outside(): void {
  try {
    platform.openOutside(address.value);
  } catch (error) {
    notify({ level: "warning", text: t("host.window.notOpened", { reason: reason(error) }) });
  }
}

const facts = computed(() =>
  opened.value
    ? [
        { term: t("host.files.fileName"), value: opened.value.name },
        { term: t("host.files.size"), value: t("host.files.bytes", { count: opened.value.size }) },
        { term: t("host.files.type"), value: opened.value.type || t("host.files.untyped") },
        { term: t("host.files.begins"), value: begins.value },
      ]
    : [],
);
const abilities = computed(() => {
  const yes = (can: boolean): string => (can ? t("host.platform.yes") : t("host.platform.no"));
  return [
    { term: t("host.platform.which"), value: t(`host.platform.${platform.platform}`) },
    { term: t("host.platform.sharedMemory"), value: yes(platform.can.sharedMemoryPreview) },
    { term: t("host.platform.tiles"), value: yes(platform.can.tilesFromDisk) },
  ];
});
</script>

<template>
  <main class="host">
    <GalleryHeader :title="t('host.title')" :intro="t('host.intro')" />
    <div class="cards">
      <Card :title="t('host.files.title')" :level="2">
        <Field :label="t('host.files.text')">
          <TextArea v-model="text" :rows="4" />
        </Field>
        <Field :label="t('host.files.name')">
          <TextField v-model="name" />
        </Field>
        <div class="actions">
          <Button icon="download" @click="save">{{ t("host.files.save") }}</Button>
          <Button icon="folder" @click="open">{{ t("host.files.open") }}</Button>
        </div>
        <KeyValue v-if="opened" data-said="opened" :entries="facts" />
      </Card>

      <Card :title="t('host.tell.title')" :level="2">
        <p class="note">{{ t("host.tell.intro") }}</p>
        <div class="actions">
          <Button @click="copy">{{ t("host.tell.copy") }}</Button>
          <Button @click="raise">{{ t("host.tell.notify") }}</Button>
        </div>
        <p v-if="told" class="note" data-said="told" role="status">{{ told }}</p>
      </Card>

      <Card :title="t('host.window.title')" :level="2">
        <Field :label="t('host.window.name')">
          <TextField v-model="title" />
        </Field>
        <Switch :model-value="full" @update:model-value="fullScreen">
          {{ t("host.window.full") }}
        </Switch>
        <Field :label="t('host.window.address')" :hint="t('host.window.addressHint')">
          <TextField v-model="address" />
        </Field>
        <div class="actions">
          <Button @click="outside">{{ t("host.window.outside") }}</Button>
        </div>
      </Card>

      <Card :title="t('host.kept.title')" :level="2">
        <Field :label="t('host.kept.label')" :hint="t('host.kept.hint')">
          <TextField v-model="remembered" />
        </Field>
      </Card>

      <Card :title="t('host.platform.title')" :level="2">
        <KeyValue data-said="platform" :entries="abilities" />
      </Card>
    </div>
  </main>
</template>

<style scoped>
.host {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr));
  gap: var(--space-6);
  align-items: start;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.note {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
