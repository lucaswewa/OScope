<script setup lang="ts">
import "@vue-flow/core/dist/style.css";
import "@vue-flow/minimap/dist/style.css";

import {
  VueFlow,
  useVueFlow,
  type Connection,
  type EdgeChange,
  type NodeChange,
} from "@vue-flow/core";
import { MiniMap } from "@vue-flow/minimap";
import { computed, shallowRef, useId, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import type { Fault, Recipe } from "@/api/pipeline";
import {
  canConnect,
  drawn,
  moved,
  withEdge,
  withoutEdge,
  withoutNodes,
  type Catalogue,
} from "@/features/workspace/editor/graph";
import GraphNode from "@/features/workspace/editor/GraphNode.vue";

/**
 * The recipe drawn as a graph (W7.2, ADR-0053), on Vue Flow: the recipe
 * draws it, and what is done to it comes back as the recipe it makes, an
 * edit. A connection between unlike ports is refused as it is dragged. The
 * selection is kept here, as the nodes are drawn again with each edit.
 */
const props = defineProps<{ recipe: Recipe; catalogue: Catalogue; faults: readonly Fault[] }>();
const emit = defineEmits<{ edit: [recipe: Recipe] }>();
const { t } = useI18n();

const flow = useId();
const { screenToFlowCoordinate, fitView } = useVueFlow(flow);
const frame = useTemplateRef<HTMLElement>("frame");

/** The nodes selected, by ID. */
const selected = shallowRef<ReadonlySet<string>>(new Set());
const view = computed(() => drawn(props.recipe, props.catalogue, props.faults));
const nodes = computed(() =>
  view.value.nodes.map((node) => ({
    id: node.id,
    type: node.kind,
    position: { x: node.x, y: node.y },
    data: node,
    // Its wrapper, a focusable group, is named so.
    ariaLabel: t(`workspace.editor.node.${node.kind}`, { id: node.id, node: node.node ?? "" }),
    deletable: node.kind === "step",
    selected: selected.value.has(node.id),
  })),
);
const edges = computed(() =>
  view.value.edges.map((edge) => ({
    id: edge.id,
    source: edge.from.node,
    sourceHandle: edge.from.port,
    target: edge.to.node,
    targetHandle: edge.to.port,
    class: `type-${edge.type}`,
  })),
);

const endsOf = (connection: Connection) => ({
  from: { node: connection.source, port: connection.sourceHandle ?? "" },
  to: { node: connection.target, port: connection.targetHandle ?? "" },
});
function valid(connection: Connection): boolean {
  const { from, to } = endsOf(connection);
  return canConnect(props.recipe, props.catalogue, from, to);
}
function connected(connection: Connection): void {
  const { from, to } = endsOf(connection);
  if (canConnect(props.recipe, props.catalogue, from, to))
    emit("edit", withEdge(props.recipe, from, to));
}

/**
 * Vue Flow gives one deletion as two changes, its edges' and its nodes', both
 * before the recipe is drawn again: each tick's changes are made to the
 * recipe the last one made, and given back as one edit, one step to undo.
 */
let pending: Recipe | undefined;
function edit(next: Recipe): void {
  if (pending === undefined)
    queueMicrotask(() => {
      const made = pending!;
      pending = undefined;
      emit("edit", made);
    });
  pending = next;
}

/** Shows the whole graph, no larger than it is drawn. */
const fit = () => fitView({ padding: 0.2, maxZoom: 1 });
// Shown whole when its nodes are first measured: as Vue Flow's own fit, but no larger.
let fitted = false;

function nodesChanged(changes: NodeChange[]): void {
  if (!fitted && changes.some((change) => change.type === "dimensions")) {
    fitted = true;
    void fit();
  }
  const was = pending ?? props.recipe;
  let next = was;
  let chosen: Set<string> | undefined;
  const removed: string[] = [];
  for (const change of changes) {
    if (change.type === "select") {
      chosen ??= new Set(selected.value);
      if (change.selected) chosen.add(change.id);
      else chosen.delete(change.id);
    } else if (change.type === "remove") removed.push(change.id);
    // A place is kept once the node is let go, or moved by a key.
    else if (change.type === "position" && change.position && !change.dragging)
      next = moved(next, change.id, [change.position.x, change.position.y]);
  }
  if (chosen) selected.value = chosen;
  if (removed.length) next = withoutNodes(next, removed);
  if (next !== was) edit(next);
}

function edgesChanged(changes: EdgeChange[]): void {
  const was = pending ?? props.recipe;
  let next = was;
  for (const change of changes) {
    if (change.type !== "remove") continue;
    const edge = view.value.edges.find((one) => one.id === change.id);
    if (edge) next = withoutEdge(next, edge.to);
  }
  if (next !== was) edit(next);
}

defineExpose({
  /** The nodes selected, by ID. */
  selected: () => [...selected.value],
  /** Selects nodes `ids`, and only them. */
  select: (ids: readonly string[]) => (selected.value = new Set(ids)),
  /** The middle of the view, in the graph's units: where a new node goes. */
  middle(): [number, number] {
    const box = frame.value?.getBoundingClientRect();
    if (!box) return [0, 0];
    const at = screenToFlowCoordinate({ x: box.x + box.width / 2, y: box.y + box.height / 2 });
    return [Math.round(at.x), Math.round(at.y)];
  },
  /** Shows the whole graph, no larger than it is drawn. */
  fit,
});
</script>

<template>
  <div ref="frame" class="graph-canvas" data-said="canvas">
    <VueFlow
      :id="flow"
      :nodes="nodes"
      :edges="edges"
      :is-valid-connection="valid"
      :delete-key-code="['Delete', 'Backspace']"
      :min-zoom="0.2"
      :max-zoom="2"
      :aria-label="t('workspace.editor.canvas')"
      @connect="connected"
      @nodes-change="nodesChanged"
      @edges-change="edgesChanged"
    >
      <template v-for="kind in ['step', 'inputs', 'outputs']" :key="kind" #[`node-${kind}`]="node">
        <GraphNode :id="node.id" :data="node.data" :selected="node.selected" />
      </template>
      <MiniMap pannable zoomable :aria-label="t('workspace.editor.minimap')" />
    </VueFlow>
  </div>
</template>

<style scoped>
.graph-canvas {
  position: relative;
  block-size: 100%;
  min-block-size: 0;
  background: var(--surface-pane);
}

/* A connection being dragged onto a port it can't join. */
.graph-canvas :deep(.vue-flow__connection-path.invalid) {
  stroke: var(--danger);
}

.graph-canvas :deep(.vue-flow__minimap) {
  border: 1px solid var(--border);
  background: var(--surface-page);
}

.graph-canvas :deep(.vue-flow__minimap-node) {
  fill: var(--text-muted);
}

.graph-canvas :deep(.vue-flow__minimap-mask) {
  fill: var(--surface-rail);
}

.graph-canvas :deep(.vue-flow__edge-path) {
  stroke: var(--text-muted);
  stroke-width: 2;
}

.graph-canvas :deep(.vue-flow__edge.type-image .vue-flow__edge-path) {
  stroke: var(--chart-1);
}

.graph-canvas :deep(.vue-flow__edge.type-stack .vue-flow__edge-path) {
  stroke: var(--chart-2);
}

.graph-canvas :deep(.vue-flow__edge.type-mask .vue-flow__edge-path) {
  stroke: var(--chart-3);
}

.graph-canvas :deep(.vue-flow__edge.type-table .vue-flow__edge-path) {
  stroke: var(--chart-4);
}

.graph-canvas :deep(.vue-flow__edge.type-geometry .vue-flow__edge-path) {
  stroke: var(--chart-5);
}

.graph-canvas :deep(.vue-flow__edge.type-json .vue-flow__edge-path) {
  stroke: var(--chart-6);
}

.graph-canvas :deep(.vue-flow__edge.selected .vue-flow__edge-path) {
  stroke-width: 4;
}

.graph-canvas :deep(.vue-flow__node) {
  padding: 0;
  border: none;
  background: none;
}
</style>
