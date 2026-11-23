<script setup lang="ts">
import { computed, onScopeDispose, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { follow, start } from "@/api/actions";
import { call, pause, server } from "@/api/client";
import { ApiError, invocationError } from "@/api/errors";
import { useProperty } from "@/api/things/use";
import { ACTIONS, commandOf, isBack, type PowerAction, type Powered } from "@/features/power/power";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Confirm from "@/ui/Confirm.vue";

/**
 * Power in the container: restart the service, confirmed by typing its name,
 * and refused while a hardware
 * job runs, unless forced. A dry run says what it would have done. After a
 * restart, the page waits for the server to be back, as the app finds it
 * again by itself.
 */
const { t } = useI18n();

const dryRunRestart = useProperty("/api/v1/power/dry_run_restart");
/** Whether `action` only says what it would do. */
const dry = (action: PowerAction) => action === "restart" && dryRunRestart.value.value === true;
const anyDry = computed(() => ACTIONS.some(dry));

/** The action whose confirmation is open, and whether it forces past a hardware job. */
const chosen = shallowRef<PowerAction>("restart");
const forcing = shallowRef(false);
const asking = shallowRef(false);
function ask(action: PowerAction, force: boolean): void {
  chosen.value = action;
  forcing.value = force;
  asking.value = true;
}

/** What the last action came to, said under the title. */
type Said =
  | { kind: "dry" | "done"; action: PowerAction; powered: Powered }
  | { kind: "refused"; action: PowerAction; why: string; forceable: boolean }
  | { kind: "restarting" | "back" | "lost"; action: "restart" };
const said = shallowRef<Said>();
const saidText = computed(() => {
  const now = said.value;
  if (!now) return "";
  if (now.kind === "dry") {
    const command = commandOf(now.powered);
    return command
      ? t("power.said.dry", { action: t(`power.${now.action}.deed`), command })
      : t("power.said.dryRestart");
  }
  if (now.kind === "done") return t(`power.said.${now.action}`);
  if (now.kind === "refused") return now.why;
  return t(`power.said.${now.kind}`);
});

const leaving = new AbortController();
onScopeDispose(() => leaving.abort());

/** How long the page waits for the server after a restart before it says so. */
const BACK_WITHIN = 120_000;

/** Waits until the server answering has started since `since`: true once it is back. */
async function back(since: number): Promise<boolean> {
  while (performance.now() - since < BACK_WITHIN) {
    await pause(1_000, leaving.signal);
    try {
      const uptime = await call((signal) => server.api.GET("/api/v1/system/uptime", { signal }), {
        timeout: 2_000,
        signal: leaving.signal,
      });
      if (isBack(uptime, performance.now() - since)) return true;
    } catch {
      // Gone, or still stopping: it is asked again.
    }
  }
  return false;
}

/**
 * How following an action ends when its server has gone: unanswered, or
 * answered by a new server that doesn't know it, after a restart.
 */
const GONE = new Set(["unreachable", "timeout", "aborted", "missing"]);

/** The action under way, if one is. */
const busy = shallowRef<PowerAction>();

/**
 * Asks for the action chosen, followed here rather than as other actions
 * are: its server may go, or come back new, before its end is seen. Its
 * refusal is said on the tab, with *Force it…* past a hardware job.
 */
async function go(): Promise<void> {
  const action = chosen.value;
  const since = performance.now();
  // A restart is said at once: the server may go before it answers.
  said.value = action === "restart" && !dry(action) ? { kind: "restarting", action } : undefined;
  busy.value = action;
  try {
    const started = await start(`/api/v1/power/${action}`, {
      confirm: true,
      force: forcing.value,
    });
    const ended = await follow(started, { signal: leaving.signal, cancelling: false });
    if (ended.status !== "completed") {
      const error = invocationError(ended);
      said.value = { kind: "refused", action, why: error.message, forceable: error.status === 409 };
      return;
    }
    const powered = ended.output as Powered;
    if (powered.dry_run) {
      said.value = { kind: "dry", action, powered };
      return;
    }
    if (action !== "restart") said.value = { kind: "done", action, powered };
  } catch (thrown) {
    if (leaving.signal.aborted) return;
    const error = thrown instanceof ApiError ? thrown : new ApiError("failed", String(thrown));
    if (!GONE.has(error.failure)) {
      said.value = { kind: "refused", action, why: error.message, forceable: false };
      return;
    }
    // Gone: rebooting or shutting down, the computer is going; restarting, it comes back.
    if (action !== "restart")
      said.value = { kind: "done", action, powered: { action, dry_run: false } };
  } finally {
    busy.value = undefined;
  }
  if (action !== "restart") return;
  said.value = { kind: "restarting", action };
  try {
    said.value = { kind: (await back(since)) ? "back" : "lost", action };
  } catch {
    // The page was left.
  }
}
</script>

<template>
  <section class="power">
    <h1>{{ t("tabs.power.title") }}</h1>
    <Banner v-if="anyDry" :title="t('power.dry.title')" data-said="dry-run">
      {{ t("power.dry.restart") }}
    </Banner>
    <div v-if="said" class="said" role="status" data-said="power" :data-kind="said.kind">
      <p>{{ saidText }}</p>
      <Button
        v-if="said.kind === 'refused' && said.forceable"
        compact
        tone="danger"
        @click="ask(said.action, true)"
      >
        {{ t("power.force") }}
      </Button>
    </div>
    <div class="actions">
      <Card
        v-for="action in ACTIONS"
        :key="action"
        :title="t(`power.${action}.title`)"
        :level="2"
        :data-action="action"
      >
        <template v-if="dry(action)" #actions>
          <Badge tone="info">{{ t("power.dryBadge") }}</Badge>
        </template>
        <p>{{ t(`power.${action}.text`) }}</p>
        <Button tone="danger" :busy="busy === action" @click="ask(action, false)">
          {{ t(`power.${action}.button`) }}
        </Button>
      </Card>
    </div>
    <Confirm
      v-model:open="asking"
      :title="t(forcing ? 'power.forcing.title' : `power.${chosen}.asking`)"
      :text="t(forcing ? 'power.forcing.text' : `power.${chosen}.warning`)"
      :confirm="t(`power.${chosen}.confirm`)"
      :typed="chosen"
      tone="danger"
      @confirm="go"
    />
  </section>
</template>

<style scoped>
.power {
  display: grid;
  gap: var(--space-4);
  align-content: start;
  max-inline-size: 60rem;
  padding: var(--space-6);
}

.power h1,
.said p,
.actions p {
  margin: 0;
}

.said {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
  padding: var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
}

.said[data-kind="refused"],
.said[data-kind="lost"] {
  border-color: var(--danger);
}

.actions {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
  gap: var(--space-4);
}

.actions :deep(.card) {
  align-content: start;
}

.actions .button {
  justify-self: start;
}
</style>
