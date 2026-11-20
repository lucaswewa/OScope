<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import TextArea from "@/ui/TextArea.vue";

/**
 * Submitting a job by its kind (W4.1), in development builds: `jobs/kinds`
 * names the kinds, without their parameters' schemas, so the parameters are
 * written as JSON.
 */
const { t } = useI18n();
const kinds = useProperty("/api/v1/jobs/kinds", { every: 60_000 });
const submit = useAction("/api/v1/jobs/submit");
const kind = shallowRef<string>();
const params = shallowRef("{}");
const choices = computed(() =>
  (kinds.value.value ?? []).map((one) => ({ value: one, label: one })),
);
const unread = computed(() => {
  try {
    JSON.parse(params.value);
    return undefined;
  } catch {
    return t("jobs.submit.notJson");
  }
});
function send(): void {
  if (!kind.value || unread.value) return;
  void submit.run({ kind: kind.value, params: JSON.parse(params.value) as unknown });
}
</script>

<template>
  <section class="submit" aria-labelledby="submit-title">
    <h2 id="submit-title">{{ t("jobs.submit.title") }}</h2>
    <Field :label="t('jobs.submit.kind')">
      <Select v-model="kind" :options="choices" />
    </Field>
    <Field :label="t('jobs.submit.params')" :error="unread">
      <TextArea v-model="params" />
    </Field>
    <Button compact :disabled="!kind || !!unread" :busy="submit.running.value" @click="send">
      {{ t("jobs.submit.send") }}
    </Button>
  </section>
</template>

<style scoped>
.submit {
  display: grid;
  gap: var(--space-2);
  justify-items: start;
}

.submit h2 {
  margin: 0;
}
</style>
