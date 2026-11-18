<script setup lang="ts">
import "./styles/layer.css";

import {
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogOverlay,
  AlertDialogPortal,
  AlertDialogRoot,
  AlertDialogTitle,
} from "reka-ui";
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import Button from "./Button.vue";
import Field from "./Field.vue";
import TextField from "./TextField.vue";

/**
 * A question that must be answered before anything else (ADR-0042): go on,
 * or not. It opens with the focus on the answer that changes nothing. Escape
 * is that answer too; a press beside it is no answer, and it stays.
 *
 * What can't be undone may ask for more than a press: with `typed`, the
 * button that goes on works only once that word has been typed, and the
 * focus starts in the field to type it in.
 */
const props = withDefaults(
  defineProps<{
    title: string;
    /** What will happen, in a sentence. */
    text: string;
    /** What the button that goes on says: the deed, not "OK". */
    confirm: string;
    /** What the other says. "Cancel", when not given. */
    cancel?: string;
    tone?: "accent" | "danger";
    /** A word to type before going on, such as the name of what will be deleted. */
    typed?: string;
  }>(),
  { tone: "accent" },
);
const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ confirm: []; cancel: [] }>();
const { t } = useI18n();
const entered = ref("");
let answered = false;

watch(open, (now) => {
  if (now) {
    entered.value = "";
    answered = false;
  } else if (!answered) {
    // Closed with no button pressed: by Escape, which is "no".
    emit("cancel");
  }
});

function answer(yes: boolean): void {
  answered = true;
  if (yes) emit("confirm");
  else emit("cancel");
}
</script>

<template>
  <AlertDialogRoot v-model:open="open">
    <AlertDialogPortal>
      <AlertDialogOverlay class="ui-scrim" />
      <AlertDialogContent class="ui-dialog narrow">
        <AlertDialogTitle class="ui-dialog-title">{{ title }}</AlertDialogTitle>
        <AlertDialogDescription class="ui-dialog-text">{{ text }}</AlertDialogDescription>
        <Field v-if="typed" :label="t('ui.confirm.type', { word: typed })">
          <TextField v-model="entered" />
        </Field>
        <footer class="ui-dialog-actions">
          <AlertDialogCancel v-if="typed === undefined" as-child>
            <Button @click="answer(false)">{{ cancel ?? t("ui.confirm.cancel") }}</Button>
          </AlertDialogCancel>
          <!-- With a word to type, the focus starts in its field, the first
               thing to use. The library would move it to the cancel button
               it knows of, so it isn't told of this one. -->
          <Button
            v-else
            @click="
              answer(false);
              open = false;
            "
          >
            {{ cancel ?? t("ui.confirm.cancel") }}
          </Button>
          <AlertDialogAction as-child>
            <Button
              variant="primary"
              :tone="tone"
              :disabled="typed !== undefined && entered.trim() !== typed"
              @click="answer(true)"
            >
              {{ props.confirm }}
            </Button>
          </AlertDialogAction>
        </footer>
      </AlertDialogContent>
    </AlertDialogPortal>
  </AlertDialogRoot>
</template>
