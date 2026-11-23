<script setup lang="ts">
import { computed, nextTick, onMounted, provide, shallowRef, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { ApiError } from "@/api/errors";
import { pipelineNodes, type NodeDescriptor } from "@/api/pipeline";
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
import { asRecipe, fileOf } from "@/features/workspace/editor/recipeFile";
import { host } from "@/host/host";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";

/**
 * The graph editor (W7.2, D12): a recipe drawn and edited as a graph. Nodes
 * from the palette; ports joined by dragging, or from the keyboard; delete,
 * duplicate, copy and paste, undo and redo, and an automatic layout. The
 * recipe and the graph are one, and its file is imported and exported.
 * `check` runs as it changes, its faults on the nodes and ports they name.
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
const { faults, checking, unchecked } = useChecking(recipe);

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
provide(EDITOR, { linking, press });

function add(node: NodeDescriptor): void {
  const at = canvas.value?.middle() ?? [0, 0];
  history.edit(withNode(recipe.value, node, at));
  said.value = t("workspace.editor.said.added", { node: node.id });
}
const selection = () => canvas.value?.selected() ?? [];
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
  canvas.value?.select(done.ids);
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
function start(): void {
  history.reset(emptyRecipe(t("workspace.editor.untitled")));
  linking.value = undefined;
}

async function importFile(): Promise<void> {
  const [file] = await host().files.open({ accept: [".json", "application/json"] });
  if (!file) return;
  try {
    const read = asRecipe(JSON.parse(await file.text()) as unknown);
    // One with no places, such as one written by hand, is laid out.
    const placed = Object.keys(read.layout ?? {}).length > 0;
    history.reset(placed ? read : laidOut(read, catalogue.value));
    said.value = t("workspace.editor.said.imported", { name: file.name });
    await nextTick();
    await canvas.value?.fit();
  } catch (thrown) {
    said.value = t("workspace.editor.said.unimported", {
      name: file.name,
      why: thrown instanceof Error ? thrown.message : String(thrown),
    });
  }
}
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
      <h2 id="editor-title">{{ recipe.name || t("workspace.editor.untitled") }}</h2>
      <Toolbar :label="t('workspace.editor.tools')">
        <ToolbarButton icon="add" @click="start">{{ t("workspace.editor.new") }}</ToolbarButton>
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
    <p v-if="unread" class="said failed" role="alert">{{ unread }}</p>
    <div class="body">
      <NodePalette :nodes="nodes" @add="add" />
      <!-- The recipe's own faults, over the canvas's corner: it keeps its size as they come and go. -->
      <div class="stage">
        <GraphCanvas
          ref="canvas"
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
    </div>
    <p class="said" aria-live="polite" data-said="editor-said">{{ said }}</p>
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

.body {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: minmax(12rem, 16rem) minmax(0, 1fr);
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
