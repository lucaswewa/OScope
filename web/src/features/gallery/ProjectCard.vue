<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ProjectSummary } from "@/api/projects";
import { descriptionOf } from "@/features/gallery/gallery";
import Icon from "@/ui/Icon.vue";
import IconButton from "@/ui/IconButton.vue";
import Menu from "@/ui/Menu.vue";
import type { MenuEntry } from "@/ui/menu";
import Tag from "@/ui/Tag.vue";

/**
 * A project's card (W6.1): its newest image, its name, its sample, its
 * description, its tags, when it was made and changed, and what it holds;
 * and a menu of its changes. The grid it is in walks it with the keys.
 */
const props = defineProps<{ project: ProjectSummary; thumbnail?: string | undefined }>();
const emit = defineEmits<{ change: [what: string] }>();
const { t } = useI18n();

const id = computed(() => `project-${props.project.id}`);
const when = (time: string) => (time ? new Date(time).toLocaleDateString() : "—");
const holds = computed(() =>
  t("projects.card.holds", {
    acquisitions: props.project.acquisitions,
    results: props.project.results,
    reports: props.project.reports,
  }),
);
const changes = computed<MenuEntry[]>(() => [
  { id: "rename", label: t("projects.change.rename"), icon: "edit" },
  { id: "tag", label: t("projects.change.tag") },
  { id: "describe", label: t("projects.change.describe") },
  { id: "sample", label: t("projects.change.sample") },
  {
    id: "delete",
    label: t("projects.change.delete"),
    icon: "delete",
    danger: true,
    separated: true,
  },
]);
</script>

<template>
  <article class="card" :aria-labelledby="id" data-said="card">
    <div class="picture">
      <img v-if="thumbnail" :src="thumbnail" alt="" />
      <Icon v-else name="gallery" class="none" />
    </div>
    <div class="about">
      <div class="top">
        <h2 :id="id" class="name">{{ project.name }}</h2>
        <Menu :items="changes" @select="(what) => emit('change', what)">
          <IconButton icon="more" :label="t('projects.card.changes', { name: project.name })" />
        </Menu>
      </div>
      <p v-if="project.sample" class="said">{{ project.sample }}</p>
      <p v-if="descriptionOf(project)" class="description">{{ descriptionOf(project) }}</p>
      <ul v-if="project.tags.length" class="tags" :aria-label="t('projects.card.tags')">
        <li v-for="tag in project.tags" :key="tag"><Tag :label="tag" /></li>
      </ul>
      <p class="said">{{ holds }}</p>
      <p class="said">
        {{
          t("projects.card.dates", { made: when(project.created), changed: when(project.modified) })
        }}
      </p>
    </div>
  </article>
</template>

<style scoped>
.card {
  display: grid;
  grid-template-rows: auto 1fr;
  block-size: 100%;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
}

.picture {
  display: grid;
  place-items: center;
  aspect-ratio: 4 / 3;
  background: var(--surface-image);
}

.picture img {
  inline-size: 100%;
  block-size: 100%;
  object-fit: cover;
}

.none {
  color: var(--text-muted);
}

.about {
  display: grid;
  gap: var(--space-1);
  align-content: start;
  padding: var(--space-2) var(--space-3) var(--space-3);
}

.top {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  justify-content: space-between;
}

.name,
.said,
.description {
  margin: 0;
}

.name {
  font-size: var(--font-size-body);
  overflow-wrap: anywhere;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
</style>
