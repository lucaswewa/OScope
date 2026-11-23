<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import type { NodeDescriptor } from "@/api/pipeline";
import { found } from "@/features/workspace/editor/palette";
import Field from "@/ui/Field.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The nodes a recipe may use (W7.2), by category, searched by name and by
 * what each does: one pressed is added to the graph.
 */
const props = defineProps<{ nodes: readonly NodeDescriptor[] }>();
const emit = defineEmits<{ add: [node: NodeDescriptor] }>();
const { t } = useI18n();

const text = shallowRef("");
const groups = computed(() => found(props.nodes, text.value));
</script>

<template>
  <nav class="palette" :aria-label="t('workspace.editor.palette.title')" data-said="palette">
    <Field :label="t('workspace.editor.palette.find')">
      <TextField v-model="text" :placeholder="t('workspace.editor.palette.findPlaceholder')" />
    </Field>
    <p v-if="!groups.length" class="said">{{ t("workspace.editor.palette.none") }}</p>
    <section
      v-for="group in groups"
      :key="group.category"
      class="group"
      :aria-labelledby="`palette-${group.category}`"
    >
      <h3 :id="`palette-${group.category}`">{{ group.category }}</h3>
      <ul>
        <!-- The button's name is what it does; the node's description, said after. -->
        <li v-for="node in group.nodes" :key="node.id" class="node">
          <button
            type="button"
            class="id"
            :aria-label="t('workspace.editor.palette.add', { node: node.id })"
            :aria-describedby="`palette-said-${node.id}`"
            @click="emit('add', node)"
          >
            {{ node.id }}
          </button>
          <span :id="`palette-said-${node.id}`" class="said">{{ node.description }}</span>
        </li>
      </ul>
    </section>
  </nav>
</template>

<style scoped>
.palette {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  overflow: auto;
}

.group h3 {
  margin: 0 0 var(--space-1);
  color: var(--text-muted);
  font-size: var(--font-size-small);
  text-transform: capitalize;
}

.group ul {
  display: grid;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* The whole entry is the button's to press, its description too. */
.node {
  position: relative;
  display: grid;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-control);
}

.node:hover {
  background: var(--surface-hover);
}

.id {
  padding: 0;
  border: none;
  background: none;
  color: inherit;
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
  text-align: start;
  cursor: pointer;
}

.id::after {
  position: absolute;
  inset: 0;
  content: "";
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
