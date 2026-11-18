<script setup lang="ts">
import {
  PaginationEllipsis,
  PaginationList,
  PaginationListItem,
  PaginationNext,
  PaginationPrev,
  PaginationRoot,
} from "reka-ui";
import { useI18n } from "vue-i18n";

import Icon from "./Icon.vue";

/**
 * Which page of many (ADR-0042): the one before and after, the first and the
 * last, and those near this one. Each is a button; the one shown says so.
 */
defineProps<{
  /** How many things in all. */
  total: number;
  /** How many to a page. */
  perPage: number;
}>();
const page = defineModel<number>({ default: 1 });
const { t } = useI18n();
</script>

<template>
  <PaginationRoot
    v-model:page="page"
    class="pagination"
    :total="total"
    :items-per-page="perPage"
    :sibling-count="1"
    show-edges
    :aria-label="t('ui.pages.label')"
  >
    <PaginationList v-slot="{ items }" class="pages">
      <PaginationPrev class="page" :aria-label="t('ui.pages.previous')">
        <Icon name="previous" small />
      </PaginationPrev>
      <template v-for="(item, at) in items" :key="at">
        <PaginationListItem
          v-if="item.type === 'page'"
          class="page"
          :value="item.value"
          :aria-label="t('ui.pages.page', { page: item.value })"
        >
          {{ item.value }}
        </PaginationListItem>
        <PaginationEllipsis v-else class="gap">…</PaginationEllipsis>
      </template>
      <PaginationNext class="page" :aria-label="t('ui.pages.next')">
        <Icon name="next" small />
      </PaginationNext>
    </PaginationList>
  </PaginationRoot>
</template>

<style scoped>
.pages {
  display: flex;
  gap: var(--space-1);
  align-items: center;
}

.page {
  display: inline-grid;
  place-content: center;
  min-inline-size: var(--size-control-compact);
  block-size: var(--size-control-compact);
  padding-inline: var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
}

.page:hover {
  background: var(--surface-hover);
}

.page[aria-current="page"] {
  border-color: var(--accent-text);
  background: var(--accent-tint);
  color: var(--accent-text);
  font-weight: var(--font-weight-strong);
}

.page:disabled {
  color: var(--text-disabled);
  cursor: not-allowed;
}

.gap {
  padding-inline: var(--space-1);
  color: var(--text-muted);
}
</style>
