<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { Recipe } from "@/api/pipeline";
import { paramsOf } from "@/features/workspace/editor/changes";
import { withParams, type Catalogue } from "@/features/workspace/editor/graph";
import Field from "@/ui/Field.vue";
import SchemaForm from "@/ui/schema-form/SchemaForm.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The editor's control pane (W7.3): the node chosen, its parameters in a
 * form drawn from its schema with its defaults, and those that differ from
 * the recipe saved. With no node chosen, the recipe's own name.
 */
const props = defineProps<{
  recipe: Recipe;
  catalogue: Catalogue;
  /** The node chosen, by ID: one step, selected. */
  chosen: string | undefined;
  /** Each node's parameters that differ from the recipe saved. */
  changed: Readonly<Record<string, string[]>>;
}>();
const emit = defineEmits<{ edit: [recipe: Recipe] }>();
const { t } = useI18n();

const used = computed(() => props.recipe.nodes.find((one) => one.id === props.chosen));
const descriptor = computed(() => used.value && props.catalogue.get(used.value.node));
const params = computed(() => (used.value ? paramsOf(used.value, props.catalogue) : {}));
const changed = computed(() => (props.chosen ? (props.changed[props.chosen] ?? []) : []));
const name = computed({
  get: () => props.recipe.name,
  set: (now: string) => emit("edit", { ...props.recipe, name: now }),
});
</script>

<template>
  <section class="editor-pane" :aria-label="t('workspace.editor.pane.title')" data-said="pane">
    <template v-if="used && descriptor">
      <h3>
        {{ used.id }} <code>{{ used.node }}</code>
      </h3>
      <p class="said">{{ descriptor.description }}</p>
      <SchemaForm
        :key="used.id"
        :model-value="params"
        :schema="descriptor.params"
        :label="t('workspace.editor.pane.params', { node: used.id })"
        @update:model-value="emit('edit', withParams(recipe, used.id, $event))"
      />
      <p class="said" :class="{ changed: changed.length }" data-said="changed">
        {{
          changed.length
            ? t("workspace.editor.pane.changed", { names: changed.join(", ") })
            : t("workspace.editor.pane.unchanged")
        }}
      </p>
    </template>
    <template v-else>
      <Field :label="t('workspace.editor.pane.name')">
        <TextField v-model="name" />
      </Field>
      <p class="said">{{ t("workspace.editor.pane.choose") }}</p>
    </template>
  </section>
</template>

<style scoped>
.editor-pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  min-inline-size: 0;
  overflow: auto;
}

h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

h3 code {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.changed {
  color: var(--accent-text);
}
</style>
