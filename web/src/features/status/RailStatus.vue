<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useConnection } from "@/api/connection/connection";
import { useLease } from "@/api/connection/lease";
import { useSession } from "@/api/connection/session";
import { useProperty } from "@/api/things/use";
import StatusIndicator from "@/features/status/StatusIndicator.vue";
import Badge from "@/ui/Badge.vue";
import Icon from "@/ui/Icon.vue";
import NoticeHistory from "@/ui/NoticeHistory.vue";
import { toasts } from "@/ui/toasts";

/**
 * The foot of the rail (W2.2): whether the server is there, who holds the
 * control lease when the server has accounts, the hardware job that runs, how
 * far it has got, leading to it (W4.1), the notices raised, with how many came
 * since they were last read, and the instrument's status.
 */
const { t } = useI18n();
const connection = useConnection();
const session = useSession();
const lease = useLease();

const tone = computed(() => {
  switch (connection.state) {
    case "connected":
      return "success";
    case "connecting":
    case "reconnecting":
      return "warning";
    default:
      return "danger";
  }
});
// The hardware job that runs, from the queue, which has its progress.
const hardware = useProperty("/api/v1/jobs/hardware", { every: 2_000 });
const queue = useProperty("/api/v1/jobs/queue", { every: 2_000 });
const running = computed(() => {
  const id = hardware.value.value;
  return id ? (queue.value.value ?? []).find((job) => job.id === id) : undefined;
});
const runningSaid = computed(() => {
  const fraction = running.value?.progress.fraction;
  return fraction === null || fraction === undefined
    ? t("status.job.short")
    : `${Math.round(fraction * 100)} %`;
});

const holder = computed(() => {
  if (session.state !== "in") return undefined;
  const held = lease.lease.value.value?.holder;
  if (!held) return t("status.lease.nobody");
  return held === session.name ? t("status.lease.you") : held;
});

// The newest notice read: those after it are new. One raised while the
// history is open is read as it comes.
const reading = ref(false);
const seen = ref(0);
const newest = computed(() => toasts.history.value[0]?.id ?? 0);
const unseen = computed(() => toasts.history.value.filter((one) => one.id > seen.value).length);
watch([reading, newest], ([open, id]) => {
  if (open) seen.value = id;
});
</script>

<template>
  <div class="rail-status">
    <p
      class="line"
      role="status"
      :data-tone="tone"
      data-said="connection"
      :title="t(`status.connection.long.${connection.state}`)"
    >
      <span class="dot" aria-hidden="true" />
      <span class="hidden">{{ t("status.connection.label") }}</span>
      <span class="text">{{ t(`status.connection.${connection.state}`) }}</span>
    </p>
    <p
      v-if="holder"
      class="line"
      data-said="holder"
      :title="t('status.lease.held', { name: holder })"
    >
      <Icon name="lock" small />
      <span class="hidden">{{ t("status.lease.label") }}</span>
      <span class="text">{{ holder }}</span>
    </p>
    <RouterLink
      v-if="running"
      class="line job"
      data-said="running-job"
      :to="{ name: 'jobs', query: { job: running.id } }"
      :title="t('status.job.long', { title: running.title })"
    >
      <Icon name="jobs" small />
      <span class="hidden">{{ t("status.job.label", { title: running.title }) }}</span>
      <span class="text">{{ runningSaid }}</span>
    </RouterLink>
    <NoticeHistory v-model:open="reading">
      <template #trigger>
        <button
          class="notices"
          type="button"
          :aria-label="
            unseen > 0 ? t('status.notices.unseen', { count: unseen }) : t('status.notices.label')
          "
        >
          <span class="bell">
            <Icon name="notifications" />
            <Badge v-if="unseen > 0" tone="accent" class="count" aria-hidden="true">{{
              unseen
            }}</Badge>
          </span>
          <span class="text">{{ t("status.notices.short") }}</span>
        </button>
      </template>
    </NoticeHistory>
    <StatusIndicator />
  </div>
</template>

<style scoped>
.rail-status {
  display: flex;
  flex-direction: column;
  border-block-start: 1px solid var(--border);
}

.line {
  display: flex;
  gap: var(--space-1);
  align-items: center;
  justify-content: center;
  margin: 0;
  padding: 1px var(--space-1);
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

/* A link, unlike the lines about it: a target a finger can take, 24 px at least. */
.job {
  min-block-size: 1.5rem;
  text-decoration: none;
}

.text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dot {
  flex: none;
  inline-size: 0.5rem;
  block-size: 0.5rem;
  border-radius: var(--radius-round);
  background: currentColor;
}

[data-tone="success"] .dot {
  background: var(--success);
}

[data-tone="warning"] .dot {
  background: var(--warning);
}

[data-tone="danger"] .dot {
  background: var(--danger);
}

.hidden {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.notices {
  display: flex;
  flex-direction: column;
  gap: 2px;
  align-items: center;
  justify-content: center;
  block-size: 2.875rem;
  padding-inline: var(--space-1);
  border: 0;
  background: none;
  color: var(--text-muted);
  font: inherit;
  font-size: var(--font-size-small);
  cursor: pointer;
}

.notices:hover {
  background: var(--surface-hover);
}

.notices:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: calc(-1 * var(--focus-width));
}

.bell {
  position: relative;
  display: inline-flex;
}

.count {
  position: absolute;
  inset-block-start: calc(-1 * var(--space-2));
  inset-inline-start: 60%;
}
</style>
