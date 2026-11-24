<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { server } from "@/api/client";
import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { send } from "@/api/transfer";
import { host } from "@/host/host";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";

/**
 * Extensions (W8.5, D11, ADR-0018): each installed package, with what it
 * brings and whether this start loaded it; one installed by uploading its
 * `.omext`, and each enabled, disabled or uninstalled, with the lease, for
 * the next start, which the page says and offers on Power; and the panels
 * of those loaded, each opened at a route of its own.
 */
const { t } = useI18n();
const installed = useProperty("/api/v1/extensions/installed", { every: 5_000 });
const panels = useProperty("/api/v1/extensions/panels", { every: 10_000 });
const host$ = useProperty("/api/v1/extensions/host", { every: 60_000 });
const restart = useProperty("/api/v1/extensions/restart_needed", { every: 5_000 });
const change = {
  enable: useAction("/api/v1/extensions/enable"),
  disable: useAction("/api/v1/extensions/disable"),
  uninstall: useAction("/api/v1/extensions/uninstall"),
};
const said = shallowRef("");
const sent = shallowRef<number>();
const extensions = thingStore("extensions");
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));

type Installed = NonNullable<typeof installed.value.value>[number];
/** What an extension brings, by kind: `nodes: a, b`. */
const brings = (one: Installed) =>
  Object.entries(one.manifest.provides)
    .filter(([, names]) => names.length > 0)
    .map(([kind, names]) => ({ kind, names: names.join(", ") }));
/** Where it stands: loaded, to be loaded, or why not. */
function standing(one: Installed): string {
  if (one.uninstalled) return t("settings.extensions.uninstalled");
  if (one.problem) return t("settings.extensions.problem", { why: one.problem });
  if (!one.enabled) return t("settings.extensions.disabled");
  if (one.loaded && one.loaded === one.next)
    return t("settings.extensions.loaded", { version: one.loaded });
  return t("settings.extensions.next", { version: one.next ?? one.manifest.version });
}
const panelsOf = (id: string) => (panels.value.value ?? []).filter((one) => one.extension === id);

async function act(which: keyof typeof change, one: Installed): Promise<void> {
  const done = await change[which].run({ id: one.manifest.id });
  const name = one.manifest.name;
  said.value =
    done === undefined && change[which].error.value
      ? t("settings.extensions.refused", { why: change[which].error.value.message })
      : t(`settings.extensions.done.${which}`, { name });
  await Promise.all(["installed", "restart_needed"].map((name) => extensions.read(name)));
}

async function install(): Promise<void> {
  const [file] = await host().files.open({ accept: [".omext"] });
  if (!file) return;
  try {
    sent.value = 0;
    const progress = (now: { done: number }) => (sent.value = now.done / Math.max(1, file.size));
    const made = (await send(server.address("/api/v1/extensions/install"), file, {
      progress,
    })) as Installed;
    said.value = t("settings.extensions.installed", {
      name: made.manifest.name,
      version: made.manifest.version,
    });
  } catch (thrown) {
    said.value = t("settings.extensions.refused", { why: why(thrown) });
  } finally {
    sent.value = undefined;
  }
  await Promise.all(["installed", "restart_needed"].map((name) => extensions.read(name)));
}

const offered = computed(() => host$.value.value);
</script>

<template>
  <section class="pane" aria-labelledby="extensions-title" data-said="extensions">
    <h2 id="extensions-title">{{ t("settings.extensions.title") }}</h2>
    <p class="note">
      {{ t("settings.extensions.note") }}
      <template v-if="offered">
        {{ t("settings.extensions.host", { api: offered.api, platform: offered.platform ?? "—" }) }}
      </template>
    </p>
    <Banner v-if="restart.value.value" level="warning" data-said="extensions-restart">
      {{ t("settings.extensions.restart") }}
      <template #actions>
        <RouterLink :to="{ name: 'power' }">{{ t("settings.systemPane.restartIt") }}</RouterLink>
      </template>
    </Banner>
    <div class="actions">
      <Button :busy="sent !== undefined" @click="install">{{
        t("settings.extensions.install")
      }}</Button>
      <Progress
        v-if="sent !== undefined"
        :label="t('settings.extensions.uploading')"
        :value="sent"
      />
    </div>
    <p v-if="(installed.value.value ?? []).length === 0" class="note">
      {{ t("settings.extensions.none") }}
    </p>
    <ul class="extensions">
      <li
        v-for="one in installed.value.value ?? []"
        :key="one.manifest.id"
        :data-extension="one.manifest.id"
      >
        <h3>
          {{ one.manifest.name }} <span class="note">{{ one.manifest.version }}</span>
        </h3>
        <p class="note">
          {{ one.manifest.publisher }} · {{ one.manifest.license }} · {{ one.manifest.kind }}
        </p>
        <p>{{ one.manifest.description }}</p>
        <dl class="brings">
          <template v-for="what in brings(one)" :key="what.kind">
            <dt>{{ t(`settings.extensions.kinds.${what.kind}`, what.kind) }}</dt>
            <dd>{{ what.names }}</dd>
          </template>
        </dl>
        <Badge
          :tone="one.problem ? 'danger' : one.loaded ? 'success' : 'neutral'"
          data-said="standing"
        >
          {{ standing(one) }}
        </Badge>
        <ul v-if="panelsOf(one.manifest.id).length" class="panels">
          <li v-for="panel in panelsOf(one.manifest.id)" :key="panel.name">
            <RouterLink
              :to="{ name: 'panel', params: { extension: one.manifest.id, panel: panel.name } }"
            >
              {{ panel.title }}
            </RouterLink>
            <span class="note">{{ panel.description }}</span>
          </li>
        </ul>
        <div class="actions">
          <Button
            v-if="one.enabled"
            compact
            :disabled="one.uninstalled"
            @click="act('disable', one)"
          >
            {{ t("settings.extensions.disable") }}
          </Button>
          <Button v-else compact :disabled="one.uninstalled" @click="act('enable', one)">
            {{ t("settings.extensions.enable") }}
          </Button>
          <Button compact :disabled="one.uninstalled" @click="act('uninstall', one)">
            {{ t("settings.extensions.uninstall") }}
          </Button>
        </div>
      </li>
    </ul>
    <p class="said" role="status" data-said="extensions-said">{{ said }}</p>
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

.extensions,
.panels {
  display: grid;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.extensions > li {
  display: grid;
  gap: var(--space-1);
  justify-items: start;
  padding: var(--space-2);
  border: 1px solid var(--border);
}

.extensions p {
  margin: 0;
}

.panels {
  gap: var(--space-1);
}

.panels li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: baseline;
}

.brings {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: var(--space-1) var(--space-2);
  margin: 0;
  font-size: var(--font-size-small);
}

.brings dt {
  color: var(--text-muted);
}

.brings dd {
  margin: 0;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
