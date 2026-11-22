<script setup lang="ts">
import { computed, nextTick, onScopeDispose, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";

import { ApiError } from "@/api/errors";
import { projectPage, projectThumbnail, type ProjectSummary } from "@/api/projects";
import { useAction, useEvent } from "@/api/things/use";
import {
  columnsOf,
  descriptionOf,
  nextCard,
  offsetOf,
  PAGE_SIZE,
  tagsOf,
  whatGoes,
} from "@/features/gallery/gallery";
import ProjectCard from "@/features/gallery/ProjectCard.vue";
import ProjectPage from "@/features/gallery/project/ProjectPage.vue";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import Dialog from "@/ui/Dialog.vue";
import EmptyState from "@/ui/EmptyState.vue";
import Field from "@/ui/Field.vue";
import Pagination from "@/ui/Pagination.vue";
import Select from "@/ui/Select.vue";
import Skeleton from "@/ui/Skeleton.vue";
import TextArea from "@/ui/TextArea.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The Gallery's projects (W6.1, D16): every project as a card, by pages,
 * found by text and by tag, sorted by date or by name, and kept live as the
 * server changes them. Each is made, renamed, tagged, described, given its
 * sample or deleted here, with the lease, a deletion confirmed with what it
 * takes. The grid is walked with the arrow keys.
 */
const { t } = useI18n();
const route = useRoute();
/** The project whose page is open, as the address says (W6.2). */
const open = computed(() =>
  typeof route.query.project === "string" ? route.query.project : undefined,
);
const isSort = (value: unknown): value is string => value === "changed" || value === "name";
const sort = kept("oscope.gallery.sort", "changed", isSort);
const text = shallowRef("");
const tag = shallowRef("");
const page = ref(1);

const projects = shallowRef<ProjectSummary[]>([]);
const total = shallowRef(0);
const loading = shallowRef(true);
const unlisted = shallowRef<string>();

let asked: AbortController | undefined;
/** The page asked for now, the last asked winning. */
async function load(): Promise<void> {
  asked?.abort();
  const mine = (asked = new AbortController());
  try {
    const listed = await projectPage(
      {
        text: text.value.trim(),
        tag: tag.value.trim(),
        sort: sort.value as "changed" | "name",
        offset: offsetOf(page.value),
        limit: PAGE_SIZE,
      },
      { signal: mine.signal },
    );
    if (mine.signal.aborted) return;
    projects.value = listed.projects;
    total.value = listed.total;
    unlisted.value = undefined;
  } catch (thrown) {
    if (mine.signal.aborted) return;
    unlisted.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  } finally {
    if (asked === mine) loading.value = false;
  }
}
let soon: ReturnType<typeof setTimeout> | undefined;
function later(): void {
  clearTimeout(soon);
  soon = setTimeout(() => void load(), 300);
}
onScopeDispose(() => {
  clearTimeout(soon);
  asked?.abort();
});
watch([text, tag, sort], () => {
  page.value = 1;
  later();
});
watch(page, () => void load(), { immediate: true });
// Kept live: a change anywhere, from any client, reads the page again; and,
// for a change told before the socket listened, every ten seconds (W6.2).
const changed = useEvent("/api/v1/projects/changed", { keep: 1 });
watch(() => changed.last.value, later);
const every = setInterval(later, 10_000);
onScopeDispose(() => clearInterval(every));

// Each card's newest image, read again as its project changes.
const thumbnails = shallowRef(new Map<string, { modified: string; url?: string }>());
watch(projects, async (now) => {
  const known = thumbnails.value;
  const next = new Map<string, { modified: string; url?: string }>();
  for (const one of now) {
    const had = known.get(one.id);
    if (had?.modified === one.modified) next.set(one.id, had);
  }
  for (const [id, had] of known) if (had.url && next.get(id) !== had) URL.revokeObjectURL(had.url);
  thumbnails.value = next;
  for (const one of now.filter((p) => !next.has(p.id))) {
    try {
      const picture = await projectThumbnail(one.id);
      const url = picture && URL.createObjectURL(picture);
      thumbnails.value = new Map(thumbnails.value).set(one.id, {
        modified: one.modified,
        ...(url ? { url } : {}),
      });
    } catch {
      // A card is said without its picture.
    }
  }
});
onScopeDispose(() => {
  for (const one of thumbnails.value.values()) if (one.url) URL.revokeObjectURL(one.url);
});

const sorts = computed(() =>
  (["changed", "name"] as const).map((value) => ({ value, label: t(`projects.sorts.${value}`) })),
);
const empty = computed(() =>
  text.value.trim() || tag.value.trim() ? t("projects.noneFound") : t("projects.none"),
);

// The grid walked with the keys: the focus moves from card to card.
const grid = ref<HTMLUListElement>();
const current = shallowRef(0);
function walk(event: KeyboardEvent): void {
  const cards = [...(grid.value?.querySelectorAll<HTMLElement>(":scope > li") ?? [])];
  const at = cards.indexOf(event.target as HTMLElement);
  if (at < 0) return;
  const columns = columnsOf(cards.map((card) => card.getBoundingClientRect().top));
  const to = nextCard(event.key, at, cards.length, columns);
  if (to === undefined) return;
  event.preventDefault();
  current.value = to;
  void nextTick(() => cards[to]?.focus());
}

// Changing: one dialog for each kind, and a question before a deletion.
const create = useAction("/api/v1/projects/create");
const rename = useAction("/api/v1/projects/rename");
const retag = useAction("/api/v1/projects/tag");
const describe = useAction("/api/v1/projects/set_metadata");
const sample = useAction("/api/v1/projects/set_sample");
const remove = useAction("/api/v1/projects/delete");
type Editing = "create" | "rename" | "tag" | "describe" | "sample";
const editing = shallowRef<{ what: Editing; project?: ProjectSummary }>();
const editingOpen = computed({
  get: () => editing.value !== undefined,
  set: (open: boolean) => {
    if (!open) editing.value = undefined;
  },
});
const typed = shallowRef("");
/** The project whose deletion is asked about: kept as the question closes, which it does first. */
const deleting = shallowRef<ProjectSummary>();
const asking = shallowRef(false);
function change(project: ProjectSummary, what: string): void {
  if (what === "delete") {
    deleting.value = project;
    asking.value = true;
    return;
  }
  const initial = {
    rename: project.name,
    tag: project.tags.join(", "),
    describe: descriptionOf(project),
    sample: project.sample,
  }[what];
  typed.value = initial ?? "";
  editing.value = { what: what as Editing, project };
}
function making(): void {
  typed.value = "";
  editing.value = { what: "create" };
}
const busy = computed(
  () =>
    create.running.value ||
    rename.running.value ||
    retag.running.value ||
    describe.running.value ||
    sample.running.value,
);
async function save(): Promise<void> {
  const now = editing.value;
  if (!now) return;
  const value = typed.value.trim();
  const id = now.project?.id ?? "";
  const done =
    now.what === "create"
      ? await create.run({ name: value })
      : now.what === "rename"
        ? await rename.run({ id, name: value })
        : now.what === "tag"
          ? await retag.run({ id, tags: tagsOf(value) })
          : now.what === "describe"
            ? await describe.run({ id, metadata: { description: value } })
            : await sample.run({ id, sample: value });
  if (done !== undefined) {
    editing.value = undefined;
    await load();
  }
}
const goes = computed(() => {
  const project = deleting.value;
  if (!project) return "";
  const parts = whatGoes(project).map((one) =>
    t(`projects.goes.${one.kind}`, { n: one.count }, one.count),
  );
  return parts.length
    ? t("projects.deleting.with", { what: parts.join(", ") })
    : t("projects.deleting.empty");
});
async function deleteIt(): Promise<void> {
  const project = deleting.value;
  if (!project) return;
  await remove.run({ id: project.id });
  await load();
}
</script>

<template>
  <ProjectPage v-if="open" :project="open" />
  <section v-else class="gallery" :aria-labelledby="'gallery-title'" data-said="gallery">
    <header class="head">
      <h1 id="gallery-title">{{ t("tabs.gallery.title") }}</h1>
      <Button icon="add" @click="making">{{ t("projects.new") }}</Button>
    </header>
    <div class="finding" role="search">
      <Field class="grow" :label="t('projects.find')">
        <TextField v-model="text" :placeholder="t('projects.findPlaceholder')" />
      </Field>
      <Field class="grow" :label="t('projects.tag')">
        <TextField v-model="tag" :placeholder="t('projects.tagPlaceholder')" />
      </Field>
      <Field :label="t('projects.sort')">
        <Select v-model="sort" :options="sorts" />
      </Field>
    </div>
    <p v-if="unlisted" class="said failed" data-said="unlisted">{{ unlisted }}</p>
    <p v-else-if="!loading" class="said" data-said="count">
      {{ t("projects.count", { n: total }, total) }}
    </p>

    <ul v-if="loading" class="grid" aria-hidden="true">
      <li v-for="n in 6" :key="n" class="skeleton"><Skeleton height="14rem" width="100%" /></li>
    </ul>
    <EmptyState v-else-if="!projects.length" :title="empty" icon="gallery" data-said="empty">
      <template #actions>
        <Button v-if="!text && !tag" @click="making">{{ t("projects.new") }}</Button>
      </template>
    </EmptyState>
    <ul
      v-else
      ref="grid"
      class="grid"
      :aria-label="t('projects.grid')"
      data-said="grid"
      @keydown="walk"
    >
      <li
        v-for="(project, i) in projects"
        :key="project.id"
        :tabindex="i === Math.min(current, projects.length - 1) ? 0 : -1"
        @focus="current = i"
      >
        <ProjectCard
          :project="project"
          :thumbnail="thumbnails.get(project.id)?.url"
          @change="(what) => change(project, what)"
        />
      </li>
    </ul>
    <Pagination
      v-if="total > PAGE_SIZE"
      v-model="page"
      :total="total"
      :per-page="PAGE_SIZE"
      data-said="pages"
    />

    <Dialog
      v-model:open="editingOpen"
      :title="
        editing ? t(`projects.editing.${editing.what}`, { name: editing.project?.name ?? '' }) : ''
      "
      narrow
    >
      <form class="edit" @submit.prevent="save">
        <Field :label="editing ? t(`projects.fields.${editing.what}`) : ''">
          <TextArea v-if="editing?.what === 'describe'" v-model="typed" />
          <TextField v-else v-model="typed" />
        </Field>
        <p v-if="editing?.what === 'tag'" class="said">{{ t("projects.tagsSaid") }}</p>
      </form>
      <template #actions>
        <Button variant="bare" @click="editing = undefined">{{ t("projects.cancel") }}</Button>
        <Button
          variant="primary"
          :busy="busy"
          :disabled="(editing?.what === 'create' || editing?.what === 'rename') && !typed.trim()"
          @click="save"
        >
          {{ t("projects.save") }}
        </Button>
      </template>
    </Dialog>
    <Confirm
      v-model:open="asking"
      :title="t('projects.deleting.title', { name: deleting?.name ?? '' })"
      :text="goes"
      :confirm="t('projects.deleting.confirm')"
      tone="danger"
      @confirm="deleteIt"
    />
  </section>
</template>

<style scoped>
.gallery {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  padding: var(--space-3);
  overflow: auto;
}

.head {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  justify-content: space-between;
}

.head h1,
.said {
  margin: 0;
}

.finding {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 12rem;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.grid > li {
  border-radius: var(--radius-surface);
}

.grid > li:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.edit {
  display: grid;
  gap: var(--space-2);
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
