<script setup lang="ts">
import { Handle, Position } from "@vue-flow/core";
import { computed, inject } from "vue";
import { useI18n } from "vue-i18n";

import { EDITOR } from "@/features/workspace/editor/editing";
import { NEW_OUTPUT, type DrawnNode } from "@/features/workspace/editor/graph";

/**
 * A node of the graph (W7.2): a step, the recipe's inputs or its outputs.
 * Its ports are coloured by their type, each a handle to drag from or to,
 * and a button for the keyboard's path. Its faults show on it, and on the
 * ports they are at.
 */
// Vue Flow gives each node many props: those not used stay off its element.
defineOptions({ inheritAttrs: false });
const props = defineProps<{ id: string; data: DrawnNode; selected?: boolean }>();
const { t } = useI18n();
const editor = inject(EDITOR)!;

const faulty = computed(() => Object.keys(props.data.faults).length > 0);
const own = computed(() => props.data.faults[""] ?? []);
const linkingHere = (port: string) =>
  editor.linking.value?.node === props.id && editor.linking.value.port === port;
const portLabel = (side: "input" | "output", port: { name: string; type: string }) =>
  t(`workspace.editor.port.${side}`, { node: props.id, port: port.name, type: port.type });
</script>

<template>
  <div class="graph-node" :class="[data.kind, { faulty, selected }]" :data-said="`node-${id}`">
    <div class="head">
      <span class="id">{{
        data.kind === "step" ? id : t(`workspace.editor.ends.${data.kind}`)
      }}</span>
      <span v-if="data.node" class="node">{{ data.node }}</span>
    </div>
    <div class="ports">
      <ul class="side inputs">
        <li
          v-for="port in data.inputs"
          :key="port.name"
          class="port"
          :class="{ faulty: data.faults[port.name] }"
          :data-input="`${id}.${port.name}`"
        >
          <Handle
            :id="port.name"
            type="target"
            :position="Position.Left"
            class="handle"
            :class="`type-${port.type}`"
          />
          <button
            type="button"
            class="name nodrag"
            :aria-label="portLabel('input', port)"
            @click="editor.press({ node: id, port: port.name }, 'input')"
          >
            {{ port.name }}<span v-if="port.optional" class="optional">?</span>
          </button>
        </li>
        <li v-if="data.kind === 'outputs'" class="port">
          <Handle :id="NEW_OUTPUT" type="target" :position="Position.Left" class="handle new" />
          <button
            type="button"
            class="name nodrag"
            :aria-label="t('workspace.editor.port.newOutput')"
            @click="editor.press({ node: id, port: NEW_OUTPUT }, 'input')"
          >
            {{ t("workspace.editor.ends.newOutput") }}
          </button>
        </li>
      </ul>
      <ul class="side outputs">
        <li
          v-for="port in data.outputs"
          :key="port.name"
          class="port"
          :class="{ linking: linkingHere(port.name), faulty: data.faults[port.name] }"
          :data-output="`${id}.${port.name}`"
        >
          <button
            type="button"
            class="name nodrag"
            :aria-label="portLabel('output', port)"
            :aria-pressed="linkingHere(port.name)"
            @click="editor.press({ node: id, port: port.name }, 'output')"
          >
            {{ port.name }}
          </button>
          <Handle
            :id="port.name"
            type="source"
            :position="Position.Right"
            class="handle"
            :class="`type-${port.type}`"
          />
        </li>
      </ul>
    </div>
    <ul v-if="faulty" class="faults" data-said="faults">
      <li v-for="message in own" :key="message">{{ message }}</li>
      <template v-for="(messages, port) in data.faults" :key="port">
        <li v-for="message in port ? messages : []" :key="message">
          <strong>{{ port }}</strong
          >: {{ message }}
        </li>
      </template>
    </ul>
  </div>
</template>

<style scoped>
.graph-node {
  min-inline-size: 10rem;
  max-inline-size: 16rem;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-control);
  background: var(--surface-raised);
  color: var(--text);
  font-size: var(--font-size-small);
}

.graph-node.selected {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.graph-node.faulty {
  border-color: var(--danger);
}

.graph-node.inputs,
.graph-node.outputs {
  background: var(--surface-rail);
}

.head {
  display: grid;
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
}

.id {
  font-weight: 600;
}

.node {
  color: var(--text-muted);
  font-family: var(--font-mono);
}

.ports {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
  padding-block: var(--space-1);
}

.side {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.port {
  position: relative;
  display: flex;
}

.outputs .port {
  justify-content: flex-end;
}

/* A target 24 px tall at the canvas's own size, as WCAG 2.2's 2.5.8 asks. */
.name {
  min-block-size: 24px;
  padding: 0 var(--space-2);
  border: none;
  border-radius: var(--radius-control);
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.name:hover {
  background: var(--surface-hover);
}

.port.linking .name {
  background: var(--accent-tint);
  color: var(--accent-text);
}

.port.faulty .name {
  color: var(--danger);
}

.optional {
  color: var(--text-muted);
}

.handle {
  inline-size: 10px;
  block-size: 10px;
  border: 2px solid var(--surface-raised);
}

.handle.type-image {
  background: var(--chart-1);
}

.handle.type-stack {
  background: var(--chart-2);
}

.handle.type-mask {
  background: var(--chart-3);
}

.handle.type-table {
  background: var(--chart-4);
}

.handle.type-geometry {
  background: var(--chart-5);
}

.handle.type-json {
  background: var(--chart-6);
}

.handle.type-report,
.handle.new {
  background: var(--text-muted);
}

.faults {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: var(--space-1) var(--space-2) var(--space-2);
  border-block-start: 1px solid var(--border);
  color: var(--danger);
  list-style: none;
}
</style>
