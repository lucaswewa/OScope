<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { useSession } from "@/api/connection/session";
import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { nameWrong, passwordWrong, spanOf } from "@/features/settings/accounts/accounts";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import Field from "@/ui/Field.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The accounts (W8.6, ADR-0055), where M1 had only the command line: each
 * listed, added and removed with the lease; one's own password changed with
 * the old one; the sessions open, and the idle times of sessions and the
 * lease. The first account turns security on at the next start, and the
 * last off: the page says so, and offers the restart on Power.
 */
const { t } = useI18n();
const session = useSession();
const names = useProperty("/api/v1/accounts/names", { every: 5_000 });
const secure = useProperty("/api/v1/accounts/secure", { every: 30_000 });
const restart = useProperty("/api/v1/accounts/restart_needed", { every: 5_000 });
const sessions = useProperty("/api/v1/accounts/sessions", { every: 5_000 });
const idle = useProperty("/api/v1/accounts/idle", { every: 5_000 });
const actions = {
  add: useAction("/api/v1/accounts/add"),
  remove: useAction("/api/v1/accounts/remove"),
  change: useAction("/api/v1/accounts/change_password"),
};
const said = shallowRef("");
const accounts = thingStore("accounts");
const refusal = (action: { error: { value: { message: string } | undefined } }) =>
  t("settings.accounts.refused", { why: action.error.value?.message ?? "" });
const read = () =>
  Promise.all(["names", "restart_needed", "sessions"].map((name) => accounts.read(name)));

/** A time, in words. */
function span(seconds: number): string {
  const { h, min, s } = spanOf(seconds);
  if (h === 0 && min === 0) return t("settings.accounts.seconds", { s });
  if (h === 0) return t("settings.accounts.minutes", { min });
  return min === 0
    ? t("settings.accounts.hoursOnly", { h })
    : t("settings.accounts.hours", { h, min });
}

// A new account, typed.
const fresh = shallowRef({ name: "", password: "", again: "" });
const freshWrong = computed(() => {
  const { name, password, again } = fresh.value;
  const named = name === "" ? undefined : nameWrong(name);
  const kept = password === "" ? undefined : passwordWrong(password, again);
  return { name: named, password: kept };
});
async function add(): Promise<void> {
  const { name, password, again } = fresh.value;
  if (nameWrong(name) || passwordWrong(password, again)) return;
  const done = await actions.add.run({ account: { name, password } });
  said.value = done === undefined ? refusal(actions.add) : t("settings.accounts.added", { name });
  if (done !== undefined) fresh.value = { name: "", password: "", again: "" };
  await read();
}

// An account to remove, asked about first. The dialog closes before it
// says it was confirmed: the name stays until the removal takes it.
const removing = shallowRef<string>();
const asking = shallowRef(false);
function ask(name: string): void {
  [removing.value, asking.value] = [name, true];
}
async function remove(): Promise<void> {
  const name = removing.value;
  removing.value = undefined;
  if (!name) return;
  const done = await actions.remove.run({ name });
  said.value =
    done === undefined ? refusal(actions.remove) : t("settings.accounts.removed", { name });
  await read();
}

// One's own password.
const own = shallowRef({ old: "", password: "", again: "" });
const ownWrong = computed(() =>
  own.value.password === "" ? undefined : passwordWrong(own.value.password, own.value.again),
);
async function change(): Promise<void> {
  const { old, password, again } = own.value;
  if (old === "" || passwordWrong(password, again)) return;
  const done = await actions.change.run({ old, password });
  said.value = done === undefined ? refusal(actions.change) : t("settings.accounts.changed");
  if (done !== undefined) own.value = { old: "", password: "", again: "" };
}

const mine = computed(() =>
  session.name && names.value.value?.includes(session.name) ? session.name : undefined,
);
</script>

<template>
  <section class="pane" aria-labelledby="accounts-title" data-said="accounts">
    <h2 id="accounts-title">{{ t("settings.accounts.title") }}</h2>
    <p class="note">{{ t("settings.accounts.note") }}</p>
    <Banner
      v-if="restart.value.value"
      level="warning"
      :title="
        secure.value.value ? t('settings.accounts.offAtStart') : t('settings.accounts.onAtStart')
      "
      data-said="accounts-restart"
    >
      {{ secure.value.value ? t("settings.accounts.offWhy") : t("settings.accounts.onWhy") }}
      <template #actions>
        <RouterLink :to="{ name: 'power' }">{{ t("settings.systemPane.restartIt") }}</RouterLink>
      </template>
    </Banner>
    <p v-if="!secure.value.value && !restart.value.value" class="note" data-said="open-server">
      {{ t("settings.accounts.open") }}
    </p>

    <h3>{{ t("settings.accounts.listTitle") }}</h3>
    <p v-if="(names.value.value ?? []).length === 0" class="note">
      {{ t("settings.accounts.none") }}
    </p>
    <ul class="names" data-said="account-names">
      <li v-for="name in names.value.value ?? []" :key="name" :data-account="name">
        <span class="name">{{ name }}</span>
        <Badge v-if="name === mine" tone="accent">{{ t("settings.accounts.you") }}</Badge>
        <Button compact @click="ask(name)">{{ t("settings.accounts.remove") }}</Button>
      </li>
    </ul>

    <form class="fields" data-said="new-account" @submit.prevent="add">
      <h3 class="whole">{{ t("settings.accounts.addTitle") }}</h3>
      <Field
        :label="t('settings.accounts.name')"
        :hint="t('settings.accounts.nameHint')"
        :error="freshWrong.name && t(`settings.accounts.wrong.${freshWrong.name}`)"
      >
        <TextField
          :model-value="fresh.name"
          autocomplete="off"
          @update:model-value="(v) => (fresh = { ...fresh, name: v })"
        />
      </Field>
      <Field
        :label="t('settings.accounts.password')"
        :error="freshWrong.password && t(`settings.accounts.wrong.${freshWrong.password}`)"
      >
        <TextField
          :model-value="fresh.password"
          secret
          autocomplete="new-password"
          @update:model-value="(v) => (fresh = { ...fresh, password: v })"
        />
      </Field>
      <Field :label="t('settings.accounts.again')">
        <TextField
          :model-value="fresh.again"
          secret
          autocomplete="new-password"
          @update:model-value="(v) => (fresh = { ...fresh, again: v })"
        />
      </Field>
      <div class="actions whole">
        <Button
          type="submit"
          variant="primary"
          :busy="actions.add.running.value"
          :disabled="!fresh.name || !fresh.password || !!freshWrong.name || !!freshWrong.password"
        >
          {{ t("settings.accounts.add") }}
        </Button>
      </div>
    </form>

    <form v-if="mine" class="fields" data-said="own-password" @submit.prevent="change">
      <h3 class="whole">{{ t("settings.accounts.ownTitle", { name: mine }) }}</h3>
      <Field :label="t('settings.accounts.old')">
        <TextField
          :model-value="own.old"
          secret
          autocomplete="current-password"
          @update:model-value="(v) => (own = { ...own, old: v })"
        />
      </Field>
      <Field
        :label="t('settings.accounts.newPassword')"
        :error="ownWrong && t(`settings.accounts.wrong.${ownWrong}`)"
      >
        <TextField
          :model-value="own.password"
          secret
          autocomplete="new-password"
          @update:model-value="(v) => (own = { ...own, password: v })"
        />
      </Field>
      <Field :label="t('settings.accounts.again')">
        <TextField
          :model-value="own.again"
          secret
          autocomplete="new-password"
          @update:model-value="(v) => (own = { ...own, again: v })"
        />
      </Field>
      <div class="actions whole">
        <Button
          type="submit"
          :busy="actions.change.running.value"
          :disabled="!own.old || !own.password || !!ownWrong"
        >
          {{ t("settings.accounts.change") }}
        </Button>
      </div>
    </form>

    <template v-if="secure.value.value">
      <h3>{{ t("settings.accounts.sessionsTitle") }}</h3>
      <p v-if="idle.value.value" class="note" data-said="idle-times">
        {{
          t("settings.accounts.idle", {
            session: span(idle.value.value.session_s),
            lease: span(idle.value.value.lease_s),
          })
        }}
        {{
          idle.value.value.lease_idle_s == null
            ? t("settings.accounts.leaseFree")
            : t("settings.accounts.leaseIdle", { idle: span(idle.value.value.lease_idle_s) })
        }}
      </p>
      <ul class="sessions" data-said="sessions">
        <li v-for="(one, i) in sessions.value.value ?? []" :key="i">
          {{ t("settings.accounts.session", { account: one.account, idle: span(one.idle_s) }) }}
        </li>
      </ul>
    </template>
    <p class="said" role="status" data-said="accounts-said">{{ said }}</p>
    <Confirm
      v-model:open="asking"
      :title="t('settings.accounts.removing', { name: removing ?? '' })"
      :text="
        removing === mine ? t('settings.accounts.removingOwn') : t('settings.accounts.removingWhy')
      "
      :confirm="t('settings.accounts.remove')"
      tone="danger"
      @confirm="remove"
    />
  </section>
</template>

<style scoped>
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.names,
.sessions {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.names li {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  padding-block: var(--space-1);
  border-block-end: 1px solid var(--border);
}

.name {
  min-inline-size: 10rem;
  font-weight: 600;
}

.fields {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  gap: var(--space-2) var(--space-3);
  align-items: start;
  max-inline-size: 48rem;
}

.whole {
  grid-column: 1 / -1;
}

.actions {
  display: flex;
  gap: var(--space-2);
}

.note,
.said,
.sessions {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
