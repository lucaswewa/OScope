<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { ListedRecipe } from "@/api/pipeline";
import { nameProblem, replaces } from "@/features/workspace/editor/library";
import Button from "@/ui/Button.vue";
import Dialog from "@/ui/Dialog.vue";
import Field from "@/ui/Field.vue";
import TextField from "@/ui/TextField.vue";

/**
 * A recipe saved under a name (W7.3): one the library may keep, checked as
 * it is typed; one of the user's own of that name is replaced, saying so.
 */
const props = defineProps<{ listed: readonly ListedRecipe[]; suggested: string }>();
const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ save: [name: string] }>();
const { t } = useI18n();

const name = shallowRef("");
watch(open, (now) => now && (name.value = props.suggested), { immediate: true });
const problem = computed(() => nameProblem(name.value, props.listed));
const replacing = computed(() => !problem.value && replaces(name.value, props.listed));

function save(): void {
  if (problem.value) return;
  emit("save", name.value);
  open.value = false;
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('workspace.editor.library.saveAs')"
    :description="t('workspace.editor.library.saveAsSaid')"
    narrow
  >
    <form data-said="save" @submit.prevent="save">
      <Field
        :label="t('workspace.editor.library.name')"
        :error="problem && t(`workspace.editor.library.problem.${problem}`)"
        :hint="replacing ? t('workspace.editor.library.replacing', { name }) : undefined"
      >
        <TextField v-model="name" autocomplete="off" />
      </Field>
    </form>
    <template #actions>
      <Button variant="outline" @click="open = false">{{ t("ui.confirm.cancel") }}</Button>
      <Button variant="primary" :disabled="!!problem" @click="save">
        {{ replacing ? t("workspace.editor.library.replace") : t("workspace.editor.save") }}
      </Button>
    </template>
  </Dialog>
</template>
