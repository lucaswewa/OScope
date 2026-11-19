<script setup lang="ts">
import { toRef } from "vue";

import { provideLocked } from "./control";

/**
 * A part of the page whose controls are locked, or freed again inside a
 * locked part (ADR-0048). The shell locks a tab while another holds the
 * control lease; a control that changes nothing on the instrument, such as
 * the theme, is freed.
 */
const props = defineProps<{ locked: boolean }>();
provideLocked(toRef(props, "locked"));
</script>

<template>
  <slot />
</template>
