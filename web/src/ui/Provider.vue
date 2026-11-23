<script setup lang="ts">
import { TooltipProvider } from "reka-ui";

import Toaster from "./Toaster.vue";
</script>

<template>
  <!-- What the components share across the whole app (ADR-0042): one tooltip
       at a time, shown after a pause, and at once after the first; and the
       notices, drawn once, here. -->
  <TooltipProvider :delay-duration="500" :skip-delay-duration="300">
    <slot />
    <Toaster />
  </TooltipProvider>
</template>
