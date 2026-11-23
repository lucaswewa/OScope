<script setup lang="ts">
import { shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { deleteRecipe, recipeList, type ListedRecipe } from "@/api/pipeline";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import Dialog from "@/ui/Dialog.vue";
import Tag from "@/ui/Tag.vue";

/**
 * The library's recipes (W7.3, ADR-0052), OScope's own first: one opened,
 * or one of the user's deleted, once asked. Read again each time it opens.
 */
const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ choose: [name: string]; deleted: [name: string] }>();
const { t } = useI18n();

const listed = shallowRef<ListedRecipe[]>([]);
const failed = shallowRef<string>();
const said = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
watch(
  open,
  async (now) => {
    if (!now) return;
    failed.value = undefined;
    try {
      listed.value = await recipeList();
    } catch (thrown) {
      failed.value = said(thrown);
    }
  },
  { immediate: true },
);

function choose(name: string): void {
  emit("choose", name);
  open.value = false;
}

/** The recipe asked about, kept past the question's closing, which may come first. */
const deleting = shallowRef<ListedRecipe>();
const asking = shallowRef(false);
function ask(one: ListedRecipe): void {
  deleting.value = one;
  asking.value = true;
}
async function remove(): Promise<void> {
  const one = deleting.value;
  if (!one) return;
  try {
    await deleteRecipe(one.name);
    listed.value = listed.value.filter((other) => other.name !== one.name);
    emit("deleted", one.name);
  } catch (thrown) {
    failed.value = said(thrown);
  }
}
</script>

<template>
  <Dialog v-model:open="open" :title="t('workspace.editor.library.title')">
    <p v-if="failed" class="failed" role="alert">{{ failed }}</p>
    <ul class="recipes" data-said="library">
      <li v-for="one in listed" :key="one.name">
        <span class="title">{{ one.title || one.name }}</span>
        <code>{{ one.name }}</code>
        <Tag v-if="one.stock" :label="t('workspace.editor.library.stock')" />
        <span class="actions">
          <Button
            compact
            :aria-label="t('workspace.editor.library.openOne', { name: one.name })"
            @click="choose(one.name)"
          >
            {{ t("workspace.editor.library.open") }}
          </Button>
          <Button
            v-if="!one.stock"
            compact
            variant="bare"
            tone="danger"
            :aria-label="t('workspace.editor.library.deleteOne', { name: one.name })"
            @click="ask(one)"
          >
            {{ t("workspace.editor.delete") }}
          </Button>
        </span>
      </li>
    </ul>
  </Dialog>
  <Confirm
    v-model:open="asking"
    :title="t('workspace.editor.library.deleteTitle')"
    :text="t('workspace.editor.library.deleteSaid', { name: deleting?.name ?? '' })"
    :confirm="t('workspace.editor.delete')"
    tone="danger"
    @confirm="remove"
  />
</template>

<style scoped>
.recipes {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.recipes li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

code {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.actions {
  display: flex;
  gap: var(--space-1);
  margin-inline-start: auto;
}

.failed {
  margin: 0;
  color: var(--danger);
}
</style>
