<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";

import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import ContextMenu from "@/ui/ContextMenu.vue";
import Dialog from "@/ui/Dialog.vue";
import EmptyState from "@/ui/EmptyState.vue";
import Field from "@/ui/Field.vue";
import Menu from "@/ui/Menu.vue";
import NoticeHistory from "@/ui/NoticeHistory.vue";
import type { MenuEntry } from "@/ui/menu";
import Popover from "@/ui/Popover.vue";
import Progress from "@/ui/Progress.vue";
import Select from "@/ui/Select.vue";
import Skeleton from "@/ui/Skeleton.vue";
import Slider from "@/ui/Slider.vue";
import Switch from "@/ui/Switch.vue";
import Tag from "@/ui/Tag.vue";
import TextField from "@/ui/TextField.vue";
import { notify, type Level } from "@/ui/toasts";

/**
 * What appears over the page, and what tells the user something happened
 * (ADR-0042): each to open from the keyboard, in both themes. What was last
 * done is written beside each, so that it can be seen, and checked.
 */
const { t } = useI18n();

const open = reactive({ dialog: false, question: false, deletion: false, popover: false });
const said = reactive({ dialog: "", question: "", deletion: "", menu: "", context: "" });
const done = ref(42);
const quick = ref(false);
const banner = ref(true);
const project = ref("histology");
const name = ref("");
const tags = ref(["H&E", "40×", "tissue"]);
let raised = 0;

const levels: Level[] = ["info", "success", "warning", "danger"];
const projects = computed(() => [
  { value: "histology", label: t("overlays.dialog.histology") },
  { value: "cytology", label: t("overlays.dialog.cytology") },
]);
const actions = computed<MenuEntry[]>(() => [
  { id: "view", label: t("overlays.menu.view"), icon: "show" },
  { id: "download", label: t("overlays.menu.download"), icon: "download" },
  { id: "rename", label: t("overlays.menu.rename"), icon: "edit", disabled: true },
  { id: "delete", label: t("overlays.menu.delete"), icon: "delete", danger: true, separated: true },
]);

/** Raises a notice of a level: quick ones go in a moment, for trying what makes one stay. */
function raise(level: Level, withAction = false): void {
  raised++;
  notify({
    level,
    text: t(`overlays.toasts.${level}`, { count: raised }),
    ...(quick.value ? { lasts: 1500 } : {}),
    ...(withAction
      ? { action: { label: t("overlays.toasts.undo"), run: () => raise("info") } }
      : {}),
  });
}
</script>

<template>
  <main class="overlays">
    <GalleryHeader :title="t('overlays.title')" :intro="t('overlays.intro')" />

    <section>
      <h2>{{ t("overlays.dialog.title") }}</h2>
      <div class="row">
        <Button @click="open.dialog = true">{{ t("overlays.dialog.open") }}</Button>
        <output data-said="dialog">{{ said.dialog }}</output>
      </div>
      <Dialog
        v-model:open="open.dialog"
        :title="t('overlays.dialog.heading')"
        :description="t('overlays.dialog.text')"
      >
        <Field :label="t('overlays.dialog.name')">
          <TextField v-model="name" />
        </Field>
        <Field :label="t('overlays.dialog.project')">
          <Select v-model="project" :options="projects" />
        </Field>
        <template #actions>
          <Button @click="open.dialog = false">{{ t("ui.confirm.cancel") }}</Button>
          <Button
            variant="primary"
            @click="
              said.dialog = t('overlays.dialog.saved', { name, project });
              open.dialog = false;
            "
          >
            {{ t("overlays.dialog.save") }}
          </Button>
        </template>
      </Dialog>

      <div class="row">
        <Button @click="open.question = true">{{ t("overlays.confirm.open") }}</Button>
        <output data-said="question">{{ said.question }}</output>
      </div>
      <Confirm
        v-model:open="open.question"
        :title="t('overlays.confirm.heading')"
        :text="t('overlays.confirm.text')"
        :confirm="t('overlays.confirm.yes')"
        :cancel="t('overlays.confirm.no')"
        @confirm="said.question = t('overlays.confirm.stopped')"
        @cancel="said.question = t('overlays.confirm.kept')"
      />

      <div class="row">
        <Button tone="danger" @click="open.deletion = true">{{ t("overlays.delete.open") }}</Button>
        <output data-said="deletion">{{ said.deletion }}</output>
      </div>
      <Confirm
        v-model:open="open.deletion"
        tone="danger"
        typed="slide-4"
        :title="t('overlays.delete.heading')"
        :text="t('overlays.delete.text')"
        :confirm="t('overlays.delete.yes')"
        @confirm="said.deletion = t('overlays.delete.deleted')"
        @cancel="said.deletion = t('overlays.delete.kept')"
      />
    </section>

    <section>
      <h2>{{ t("overlays.popover.title") }}</h2>
      <div class="row">
        <Popover v-model:open="open.popover">
          <template #trigger>
            <Button icon="settings">{{ t("overlays.popover.open") }}</Button>
          </template>
          <div class="panel">
            <Switch v-model="quick">{{ t("overlays.toasts.quick") }}</Switch>
            <Field :label="t('overlays.progress.done')">
              <Slider v-model="done" />
            </Field>
          </div>
        </Popover>
      </div>
    </section>

    <section>
      <h2>{{ t("overlays.menu.title") }}</h2>
      <div class="row">
        <Menu :items="actions" @select="(id) => (said.menu = id)">
          <Button icon="more">{{ t("overlays.menu.open") }}</Button>
        </Menu>
        <output data-said="menu">{{ said.menu }}</output>
      </div>
      <div class="row">
        <ContextMenu :items="actions" @select="(id) => (said.context = id)">
          <div class="target" tabindex="0">{{ t("overlays.menu.target") }}</div>
        </ContextMenu>
        <output data-said="context">{{ said.context }}</output>
      </div>
    </section>

    <section>
      <h2>{{ t("overlays.toasts.title") }}</h2>
      <div class="row">
        <Button v-for="level in levels" :key="level" @click="raise(level)">
          {{ t(`overlays.toasts.raise.${level}`) }}
        </Button>
        <Button @click="raise('success', true)">{{ t("overlays.toasts.withAction") }}</Button>
        <Switch v-model="quick">{{ t("overlays.toasts.quick") }}</Switch>
        <NoticeHistory>
          <template #trigger>
            <Button icon="notifications">{{ t("overlays.toasts.history") }}</Button>
          </template>
        </NoticeHistory>
      </div>
    </section>

    <section>
      <h2>{{ t("overlays.banners.title") }}</h2>
      <Banner>{{ t("overlays.banners.info") }}</Banner>
      <Banner level="success" :title="t('overlays.banners.successTitle')">
        {{ t("overlays.banners.success") }}
      </Banner>
      <Banner level="warning">
        {{ t("overlays.banners.warning") }}
        <template #actions>
          <Button compact>{{ t("overlays.banners.act") }}</Button>
        </template>
      </Banner>
      <Banner v-if="banner" level="danger" dismissable @dismiss="banner = false">
        {{ t("overlays.banners.danger") }}
      </Banner>
    </section>

    <section>
      <h2>{{ t("overlays.progress.title") }}</h2>
      <div class="grid">
        <Field :label="t('overlays.progress.known')">
          <Progress :value="done" />
        </Field>
        <Field :label="t('overlays.progress.unknown')">
          <Progress />
        </Field>
        <Field :label="t('overlays.progress.done')">
          <Slider v-model="done" unit="%" />
        </Field>
      </div>
      <div class="row">
        <Progress ring :value="done" :label="t('overlays.progress.known')" />
        <Progress ring :label="t('overlays.progress.unknown')" />
        <output data-said="done">{{ done }}</output>
      </div>
    </section>

    <section>
      <h2>{{ t("overlays.rest.title") }}</h2>
      <div class="grid" aria-busy="true">
        <div class="lines">
          <Skeleton height="6rem" />
          <Skeleton width="60%" />
          <Skeleton width="85%" />
        </div>
        <EmptyState class="none" icon="folder" :title="t('overlays.rest.emptyTitle')">
          {{ t("overlays.rest.empty") }}
          <template #actions>
            <Button variant="primary" icon="camera">{{ t("overlays.rest.capture") }}</Button>
          </template>
        </EmptyState>
      </div>
      <div class="row">
        <Badge>12</Badge>
        <Badge tone="accent">{{ t("overlays.rest.new") }}</Badge>
        <Badge tone="info">{{ t("overlays.rest.queued") }}</Badge>
        <Badge tone="success">{{ t("overlays.rest.done") }}</Badge>
        <Badge tone="warning">{{ t("overlays.rest.paused") }}</Badge>
        <Badge tone="danger">{{ t("overlays.rest.failed") }}</Badge>
      </div>
      <div class="row">
        <Tag
          v-for="tag in tags"
          :key="tag"
          :label="tag"
          removable
          @remove="tags = tags.filter((one) => one !== tag)"
        />
        <Tag :label="t('overlays.rest.fixed')" />
        <output data-said="tags">{{ tags.join(", ") }}</output>
      </div>
    </section>
  </main>
</template>

<style scoped>
.overlays {
  display: grid;
  gap: var(--space-8);
  max-inline-size: 72rem;
  padding: var(--space-6);
}

section {
  display: grid;
  gap: var(--space-3);
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, 22rem);
  gap: var(--space-4);
  align-items: start;
}

.panel {
  display: grid;
  gap: var(--space-3);
  inline-size: 18rem;
}

.target {
  display: grid;
  place-content: center;
  inline-size: 22rem;
  block-size: var(--size-rail-item);
  border: 1px dashed var(--border-control);
  border-radius: var(--radius-surface);
  color: var(--text-muted);
}

.lines {
  display: grid;
  gap: var(--space-2);
}

.none {
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
}

output {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
