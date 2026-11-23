<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  provide,
  shallowRef,
  useTemplateRef,
  watch,
} from "vue";
import { useI18n } from "vue-i18n";
import { onBeforeRouteLeave } from "vue-router";

import { ApiError } from "@/api/errors";
import {
  liveRecipe,
  pipelineNodes,
  recipeList,
  recipeNamed,
  saveRecipe,
  setLiveRecipe,
  type ListedRecipe,
  type LiveCheck,
  type NodeDescriptor,
  type Recipe,
} from "@/api/pipeline";
import LiveViewer from "@/features/live/LiveViewer.vue";
import { changesOf } from "@/features/workspace/editor/changes";
import { costOf, costsOf } from "@/features/workspace/editor/costs";
import EditorPane from "@/features/workspace/editor/EditorPane.vue";
import { EDITOR, useChecking } from "@/features/workspace/editor/editing";
import GraphCanvas from "@/features/workspace/editor/GraphCanvas.vue";
import {
  catalogueOf,
  copied,
  emptyRecipe,
  freeOutput,
  NEW_OUTPUT,
  pasted,
  pressPort,
  withEdge,
  withNode,
  withoutNodes,
  type Copied,
  type End,
  type Linking,
} from "@/features/workspace/editor/graph";
import { useHistory } from "@/features/workspace/editor/history";
import { laidOut } from "@/features/workspace/editor/layout";
import NodePalette from "@/features/workspace/editor/NodePalette.vue";
import OpenDialog from "@/features/workspace/editor/OpenDialog.vue";
import { asRecipe, fileOf } from "@/features/workspace/editor/recipeFile";
import SaveDialog from "@/features/workspace/editor/SaveDialog.vue";
import { nameFor } from "@/features/workspace/editor/library";
import { host } from "@/host/host";
import Confirm from "@/ui/Confirm.vue";
import Splitter from "@/ui/Splitter.vue";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";

/**
 * The graph editor (W7.2, D12): a recipe drawn and edited as a graph. Nodes
 * from the palette; ports joined by dragging, or from the keyboard; delete,
 * duplicate, copy and paste, undo and redo, and an automatic layout. The
 * recipe and the graph are one, and its file is imported and exported.
 * `check` runs as it changes, its faults on the nodes and ports they name.
 *
 * W7.3: a node's parameters in the control pane; the library, its recipes
 * opened, saved and deleted, unsaved changes asked about before they are
 * left; and the recipe run live as it changes, each node's cost on it.
 */
const { t } = useI18n();
const history = useHistory(emptyRecipe(t("workspace.editor.untitled")));
const recipe = history.now;
const canvas = useTemplateRef<InstanceType<typeof GraphCanvas>>("canvas");

const nodes = shallowRef<NodeDescriptor[]>([]);
const unread = shallowRef<string>();
onMounted(async () => {
  try {
    nodes.value = await pipelineNodes();
  } catch (thrown) {
    unread.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  }
});
const catalogue = computed(() => catalogueOf(nodes.value));
const { faults, checking, unchecked, checked } = useChecking(recipe);
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));

/** The nodes selected, by ID; one step alone is the one the pane shows. */
const selected = shallowRef<ReadonlySet<string>>(new Set());
const chosen = computed(() => {
  const [id, ...more] = selected.value;
  return !more.length && recipe.value.nodes.some((used) => used.id === id) ? id : undefined;
});

// The library: the recipe as it was last opened, saved or begun, and the
// library's name for it, if it has one.
const base = shallowRef<Recipe>(recipe.value);
const kept = shallowRef<{ name: string; stock: boolean }>();
const changes = computed(() => changesOf(base.value, recipe.value, catalogue.value));
/** `next` made the editor's own: nothing to undo, nothing unsaved. */
function begin(next: Recipe, from?: { name: string; stock: boolean }): void {
  history.reset(next);
  base.value = next;
  kept.value = from;
  linking.value = undefined;
  selected.value = new Set();
}

// Unsaved changes, asked about before they are left: for another recipe,
// or another tab; and the browser's own question, for a page closed.
const leaving = shallowRef(false);
let answer: ((go: boolean) => void) | undefined;
function mayLeave(): Promise<boolean> {
  if (!changes.value.unsaved) return Promise.resolve(true);
  leaving.value = true;
  return new Promise((resolve) => (answer = resolve));
}
function answered(go: boolean): void {
  answer?.(go);
  answer = undefined;
}
const unlessKept = (action: () => unknown) => async () => {
  if (await mayLeave()) await action();
};
onBeforeRouteLeave(mayLeave);
function unloading(event: BeforeUnloadEvent): void {
  if (changes.value.unsaved) event.preventDefault();
}
onMounted(() => window.addEventListener("beforeunload", unloading));
onBeforeUnmount(() => window.removeEventListener("beforeunload", unloading));

const opening = shallowRef(false);
const saving = shallowRef(false);
const listed = shallowRef<ListedRecipe[]>([]);
async function openNamed(name: string): Promise<void> {
  try {
    const found = await recipeNamed(name);
    const placed = Object.keys(found.recipe.layout ?? {}).length > 0;
    begin(placed ? found.recipe : laidOut(found.recipe, catalogue.value), found);
    said.value = t("workspace.editor.said.opened", { name });
    await nextTick();
    await canvas.value?.fit();
  } catch (thrown) {
    said.value = t("workspace.editor.said.unopened", { name, why: why(thrown) });
  }
}
const open = unlessKept(() => (opening.value = true));
async function saveAs(name: string): Promise<void> {
  const now = recipe.value;
  try {
    await saveRecipe(name, now);
    base.value = now;
    kept.value = { name, stock: false };
    said.value = t("workspace.editor.said.saved", { name });
  } catch (thrown) {
    said.value = t("workspace.editor.said.unsaved", { name, why: why(thrown) });
  }
}
/** Saved under a name asked for: a new recipe's, a stock one's copy, or another. */
async function askName(): Promise<void> {
  listed.value = await recipeList().catch(() => []);
  saving.value = true;
}
/** Saved in place, when it has a name of the user's. */
async function save(): Promise<void> {
  await (kept.value && !kept.value.stock ? saveAs(kept.value.name) : askName());
}
function deleted(name: string): void {
  if (kept.value?.name === name) kept.value = undefined;
  said.value = t("workspace.editor.said.deleted", { name });
}

// Live: the recipe set as the preview's each time it is checked and can
// run; and what was live before set again when it stops, or is left.
const living = shallowRef(false);
const live = shallowRef<LiveCheck>();
const unlive = shallowRef<string>();
let before: Recipe | null = null;
let sent: Recipe | undefined;
async function send(): Promise<void> {
  const now = recipe.value;
  if (!living.value || now === sent || checked.value !== now || faults.value.length) return;
  sent = now;
  try {
    live.value = (await setLiveRecipe(now)) ?? undefined;
    unlive.value = undefined;
  } catch (thrown) {
    unlive.value = why(thrown);
  }
}
watch([living, checked], () => void send());
async function toggleLive(): Promise<void> {
  if (living.value) {
    living.value = false;
    live.value = unlive.value = sent = undefined;
    await setLiveRecipe(before).catch(() => undefined);
    return;
  }
  try {
    before = await liveRecipe();
    living.value = true;
  } catch (thrown) {
    unlive.value = why(thrown);
  }
}
onBeforeUnmount(() => {
  if (living.value) void setLiveRecipe(before).catch(() => undefined);
});
const costs = computed(() =>
  living.value && live.value ? costsOf(live.value.step_ms, live.value.budget_ms) : new Map(),
);
const liveSaid = computed(() => {
  if (unlive.value) return t("workspace.editor.live.refused", { why: unlive.value });
  if (!living.value) return "";
  if (!live.value) return t("workspace.editor.live.waiting");
  const whole = costOf(live.value.time_ms, live.value.budget_ms);
  return t("workspace.editor.live.cost", { ...whole, budget: live.value.budget_ms });
});
const liveSizes = shallowRef<[number, number]>([55, 45]);

/** What the editor last said: an edit, or the keyboard's connection. */
const said = shallowRef("");
const status = computed(() => {
  if (unchecked.value) return t("workspace.editor.checked.unchecked", { why: unchecked.value });
  if (checking.value) return t("workspace.editor.checked.checking");
  const n = faults.value.length;
  return n ? t("workspace.editor.checked.faults", { n }, n) : t("workspace.editor.checked.runs");
});
/** The faults at no node: the recipe's own, its outputs'. */
const loose = computed(() => faults.value.filter((fault) => !fault.node));

// The keyboard's path: an output pressed, then an input.
const linking = shallowRef<Linking>();
function press(end: End, side: "input" | "output"): void {
  const done = pressPort(recipe.value, catalogue.value, linking.value, end, side);
  linking.value = done.linking;
  const from = done.join?.from ?? done.linking ?? end;
  // A new output is said by the name the join gives it.
  const to = done.join && end.port === NEW_OUTPUT ? freeOutput(recipe.value, from.port) : end.port;
  if (done.join) history.edit(withEdge(recipe.value, done.join.from, done.join.to));
  said.value = t(`workspace.editor.linking.${done.said}`, {
    from: `${from.node}.${from.port}`,
    to: `${end.node}.${to}`,
  });
}
provide(EDITOR, { linking, press, costs });

function add(node: NodeDescriptor): void {
  const at = canvas.value?.middle() ?? [0, 0];
  history.edit(withNode(recipe.value, node, at));
  said.value = t("workspace.editor.said.added", { node: node.id });
}
const selection = () => [...selected.value];
function remove(): void {
  const ids = selection();
  if (!ids.length) return;
  history.edit(withoutNodes(recipe.value, ids));
  said.value = t("workspace.editor.said.removed", { n: ids.length }, ids.length);
}
let clipboard: Copied | undefined;
function copy(): void {
  const ids = selection();
  if (!ids.length) return;
  clipboard = copied(recipe.value, ids);
  said.value = t("workspace.editor.said.copied", { n: ids.length }, ids.length);
}
function paste(): void {
  if (!clipboard?.nodes.length) return;
  const done = pasted(recipe.value, clipboard);
  history.edit(done.recipe);
  selected.value = new Set(done.ids);
  said.value = t("workspace.editor.said.pasted", { n: done.ids.length }, done.ids.length);
}
function duplicate(): void {
  copy();
  paste();
}
async function layOut(): Promise<void> {
  history.edit(laidOut(recipe.value, catalogue.value));
  // Shown whole once it is drawn where it is laid out.
  await nextTick();
  await canvas.value?.fit();
}
const start = unlessKept(() => begin(emptyRecipe(t("workspace.editor.untitled"))));

const importFile = unlessKept(async () => {
  const [file] = await host().files.open({ accept: [".json", "application/json"] });
  if (!file) return;
  try {
    const read = asRecipe(JSON.parse(await file.text()) as unknown);
    // One with no places, such as one written by hand, is laid out.
    const placed = Object.keys(read.layout ?? {}).length > 0;
    begin(placed ? read : laidOut(read, catalogue.value));
    said.value = t("workspace.editor.said.imported", { name: file.name });
    await nextTick();
    await canvas.value?.fit();
  } catch (thrown) {
    said.value = t("workspace.editor.said.unimported", {
      name: file.name,
      why: why(thrown),
    });
  }
});
async function exportFile(): Promise<void> {
  const name = `${(recipe.value.name || "recipe").replace(/[^\w-]+/g, "-")}.json`;
  await host().files.save(name, new Blob([fileOf(recipe.value)], { type: "application/json" }));
}

/** The editor's chords, while the focus is in it: undo, redo, copy, paste, duplicate. */
function keys(event: KeyboardEvent): void {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
  const inField = (event.target as HTMLElement | null)?.closest("input, textarea");
  if (inField) return;
  const key = event.key.toLowerCase();
  const run: Record<string, () => void> = {
    z: event.shiftKey ? history.redo : history.undo,
    y: history.redo,
    c: copy,
    v: paste,
    d: duplicate,
  };
  const action = run[key];
  if (!action) return;
  event.preventDefault();
  action();
}
function escape(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !linking.value) return;
  linking.value = undefined;
  said.value = t("workspace.editor.linking.cancelled");
}
</script>

<template>
  <section
    class="recipe-editor"
    aria-labelledby="editor-title"
    data-said="editor"
    @keydown="keys"
    @keydown.capture="escape"
  >
    <header class="head">
      <h2 id="editor-title">
        {{ recipe.name || t("workspace.editor.untitled") }}
        <span class="kept" data-said="kept">{{
          t(`workspace.editor.kept.${!kept ? "none" : kept.stock ? "stock" : "user"}`, {
            name: kept?.name ?? "",
          }) + (changes.unsaved ? t("workspace.editor.kept.unsaved") : "")
        }}</span>
      </h2>
      <Toolbar :label="t('workspace.editor.tools')">
        <ToolbarButton icon="add" @click="start">{{ t("workspace.editor.new") }}</ToolbarButton>
        <ToolbarButton @click="open">{{ t("workspace.editor.library.open") }}…</ToolbarButton>
        <ToolbarButton :disabled="!changes.unsaved && !!kept && !kept.stock" @click="save">
          {{ t("workspace.editor.save") }}
        </ToolbarButton>
        <ToolbarButton @click="askName">{{ t("workspace.editor.saveAs") }}</ToolbarButton>
        <ToolbarButton icon="upload" @click="importFile">
          {{ t("workspace.editor.import") }}
        </ToolbarButton>
        <ToolbarButton icon="download" @click="exportFile">
          {{ t("workspace.editor.export") }}
        </ToolbarButton>
        <ToolbarButton :disabled="!history.canUndo.value" @click="history.undo">
          {{ t("workspace.editor.undo") }}
        </ToolbarButton>
        <ToolbarButton :disabled="!history.canRedo.value" @click="history.redo">
          {{ t("workspace.editor.redo") }}
        </ToolbarButton>
        <ToolbarButton @click="duplicate">{{ t("workspace.editor.duplicate") }}</ToolbarButton>
        <ToolbarButton icon="delete" @click="remove">{{
          t("workspace.editor.delete")
        }}</ToolbarButton>
        <ToolbarButton @click="layOut">{{ t("workspace.editor.layOut") }}</ToolbarButton>
        <ToolbarButton icon="fit" @click="canvas?.fit()">{{
          t("workspace.editor.fit")
        }}</ToolbarButton>
        <ToolbarButton @click="canvas?.actual()">{{ t("workspace.editor.actual") }}</ToolbarButton>
        <ToolbarButton :pressed="living" @click="toggleLive">
          {{ t("workspace.editor.live.title") }}
        </ToolbarButton>
      </Toolbar>
    </header>
    <p
      class="said"
      :class="{ failed: faults.length || unchecked }"
      role="status"
      data-said="checked"
    >
      {{ status }}
    </p>
    <p v-if="liveSaid" class="said" :class="{ failed: unlive }" data-said="live">{{ liveSaid }}</p>
    <p v-if="unread" class="said failed" role="alert">{{ unread }}</p>
    <div class="body">
      <NodePalette :nodes="nodes" @add="add" />
      <!-- The recipe's own faults, over the canvas's corner: it keeps its size as they come and go. -->
      <div class="stage">
        <Splitter
          v-if="living"
          v-model:sizes="liveSizes"
          class="live"
          :label="t('workspace.editor.live.handle')"
          :min="[30, 20]"
        >
          <template #first>
            <GraphCanvas
              ref="canvas"
              v-model:selected="selected"
              :recipe="recipe"
              :catalogue="catalogue"
              :faults="faults"
              @edit="history.edit"
            />
          </template>
          <template #second><LiveViewer /></template>
        </Splitter>
        <GraphCanvas
          v-else
          ref="canvas"
          v-model:selected="selected"
          :recipe="recipe"
          :catalogue="catalogue"
          :faults="faults"
          @edit="history.edit"
        />
        <ul v-if="loose.length" class="loose" data-said="recipe-faults">
          <li v-for="fault in loose" :key="`${fault.at}: ${fault.message}`">
            <code>{{ fault.at }}</code> {{ fault.message }}
          </li>
        </ul>
      </div>
      <EditorPane
        :recipe="recipe"
        :catalogue="catalogue"
        :chosen="chosen"
        :changed="changes.params"
        @edit="history.edit"
      />
    </div>
    <p class="said" aria-live="polite" data-said="editor-said">{{ said }}</p>
    <OpenDialog v-model:open="opening" @choose="openNamed" @deleted="deleted" />
    <SaveDialog
      v-model:open="saving"
      :listed="listed"
      :suggested="kept && !kept.stock ? kept.name : nameFor(recipe.name)"
      @save="saveAs"
    />
    <Confirm
      v-model:open="leaving"
      :title="t('workspace.editor.leave.title')"
      :text="t('workspace.editor.leave.text')"
      :confirm="t('workspace.editor.leave.confirm')"
      tone="danger"
      @confirm="answered(true)"
      @cancel="answered(false)"
    />
  </section>
</template>

<style scoped>
.recipe-editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  block-size: 100%;
  min-block-size: 0;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: center;
  justify-content: space-between;
}

.head h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.kept {
  margin-inline-start: var(--space-2);
  color: var(--text-muted);
  font-size: var(--font-size-small);
  font-weight: normal;
}

.body {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: minmax(12rem, 16rem) minmax(0, 1fr) minmax(14rem, 18rem);
  gap: var(--space-2);
  min-block-size: 24rem;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}

.stage {
  position: relative;
  min-inline-size: 0;
  min-block-size: 0;
}

.live {
  block-size: 100%;
}

.loose {
  position: absolute;
  z-index: 5;
  inset-block-start: var(--space-2);
  inset-inline-start: var(--space-2);
  max-inline-size: calc(100% - 2 * var(--space-2));
  margin: 0;
  padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  border: 1px solid var(--danger);
  border-radius: var(--radius-control);
  background: var(--surface-raised);
  color: var(--danger);
  font-size: var(--font-size-small);
}
</style>
