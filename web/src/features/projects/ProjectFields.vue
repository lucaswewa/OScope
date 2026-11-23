<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { ProjectChoice } from "@/features/projects/projectChoice";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";

/** A project chosen from those listed, or a new one made by its name (W3.7, W5.2). */
const props = defineProps<{ choice: ProjectChoice }>();
const { t } = useI18n();
const { project, choices, unlisted, naming, making, makeProject } = props.choice;
</script>

<template>
  <Field :label="t('projectChoice.project')" :error="unlisted">
    <Select v-model="project" :options="choices" :placeholder="t('projectChoice.none')" />
  </Field>
  <div class="row">
    <Field class="grow" :label="t('projectChoice.new')">
      <TextField
        v-model="naming"
        :placeholder="t('projectChoice.name')"
        @keydown.enter="makeProject"
      />
    </Field>
    <Button compact :busy="making" :disabled="!naming.trim()" @click="makeProject">
      {{ t("projectChoice.make") }}
    </Button>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 9rem;
}
</style>
