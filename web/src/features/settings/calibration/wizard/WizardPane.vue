<script setup lang="ts">
import { computed, onScopeDispose, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { job as jobNow, JOB_ENDED, type Job } from "@/api/jobs";
import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import ObjectivePane from "@/features/control/objective/ObjectivePane.vue";
import LiveViewer from "@/features/live/LiveViewer.vue";
import { shownOf, type ShownStep } from "@/features/settings/calibration/wizard/wizard";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";

/**
 * The calibration wizard (W8.3, P33): a new objective calibrated by
 * following its steps. Each says what to do, then measures with a job,
 * beside the live image, and says what it found; a failed one is tried
 * again or skipped. The wizard is the server's, kept across a restart, so
 * a reload, or another browser with the lease, finds it where it was. At
 * its end, each step's set, in use or chosen to be.
 */
const { t } = useI18n();
const wizard = useProperty("/api/v1/calibration/wizard", { every: 1_000 });
const sets = useProperty("/api/v1/calibration/sets", { every: 5_000 });
const actions = {
  start: useAction("/api/v1/calibration/wizard_start"),
  confirm: useAction("/api/v1/calibration/wizard_confirm"),
  retry: useAction("/api/v1/calibration/wizard_retry"),
  skip: useAction("/api/v1/calibration/wizard_skip"),
  cancel: useAction("/api/v1/calibration/wizard_cancel"),
};
const activate = useAction("/api/v1/calibration/activate");
type Act = keyof typeof actions;

const shown = computed(() => (wizard.value.value ? shownOf(wizard.value.value) : undefined));
const busy = computed(() => Object.values(actions).some((one) => one.running.value));
const refused = shallowRef("");
const calibration = thingStore("calibration");

async function act(which: Act): Promise<void> {
  refused.value = "";
  const action = actions[which];
  const done = await (which === "start" ? actions.start.run({}) : action.run({} as never));
  if (done === undefined) refused.value = action.error.value?.message ?? "";
  await Promise.all(["wizard", "sets", "active"].map((name) => calibration.read(name)));
}

/** What a step found, in words: its set's headline, or the server's. */
function found(step: ShownStep): string | undefined {
  if (step.set) return sets.value.value?.find((one) => one.id === step.set)?.headline;
  return step.said;
}
const inUse = (id: string) => sets.value.value?.find((one) => one.id === id)?.active === true;
async function use(id: string): Promise<void> {
  if (await activate.run({ id })) await calibration.read("sets");
}

// The running step's job, read twice a second while it runs.
const job = shallowRef<Job>();
let timer: ReturnType<typeof setTimeout> | undefined;
const stop = () => clearTimeout(timer);
watch(
  () => shown.value?.step?.job,
  (id) => {
    stop();
    job.value = undefined;
    const look = async (): Promise<void> => {
      if (!id) return;
      try {
        const now = await jobNow(id);
        if (id !== shown.value?.step?.job) return;
        job.value = now;
        // Ended, the step settles, and its set is read at once for what it found.
        if (JOB_ENDED.has(now.state))
          await Promise.all([calibration.read("wizard"), calibration.read("sets")]);
        else timer = setTimeout(() => void look(), 500);
      } catch {
        timer = setTimeout(() => void look(), 1_000);
      }
    };
    void look();
  },
  { immediate: true },
);
onScopeDispose(stop);
</script>

<template>
  <section class="pane" aria-labelledby="wizard-title" data-said="wizard">
    <h2 id="wizard-title">{{ t("settings.calibration.wizard.title") }}</h2>
    <p class="note">{{ t("settings.calibration.wizard.note") }}</p>
    <div class="body" :class="{ under: shown && !shown.ended }">
      <div class="steps-and-step">
        <ol v-if="shown" class="steps" data-said="wizard-steps">
          <li
            v-for="(step, i) in shown.steps"
            :key="step.kind"
            :class="{ current: step.current }"
            :aria-current="step.current ? 'step' : undefined"
          >
            <span class="number">{{ i + 1 }}</span>
            <span class="title">{{ step.title }}</span>
            <Badge
              :tone="
                step.standing === 'failed'
                  ? 'danger'
                  : step.standing === 'done'
                    ? 'success'
                    : 'neutral'
              "
            >
              {{ t(`settings.calibration.wizard.standings.${step.standing}`) }}
            </Badge>
            <template v-if="shown.ended && step.set">
              <Badge v-if="inUse(step.set)" tone="accent">
                {{ t("settings.calibration.active") }}
              </Badge>
              <Button v-else compact @click="use(step.set)">
                {{ t("settings.calibration.use") }}
              </Button>
            </template>
            <span v-if="found(step)" class="found">{{ found(step) }}</span>
          </li>
        </ol>
        <section v-if="shown?.step" class="step" data-said="wizard-step">
          <h3>
            {{
              t("settings.calibration.wizard.stepOf", {
                n: shown.steps.indexOf(shown.step) + 1,
                count: shown.steps.length,
                title: shown.step.title,
              })
            }}
          </h3>
          <p class="prompt">{{ wizard.value.value?.steps[wizard.value.value.current]?.prompt }}</p>
          <ObjectivePane v-if="shown.step.kind === 'objective'" />
          <Progress
            v-if="shown.step.standing === 'running'"
            :label="job?.progress.stage || t('settings.calibration.wizard.measuring')"
            :value="job?.progress.fraction ?? undefined"
          />
          <p v-if="shown.step.standing === 'failed'" class="failed" role="alert">
            {{ t("settings.calibration.wizard.failed", { why: shown.step.said ?? "" }) }}
          </p>
          <div class="buttons">
            <Button v-if="shown.can.confirm" variant="primary" :busy="busy" @click="act('confirm')">
              {{ t("settings.calibration.wizard.confirm") }}
            </Button>
            <Button v-if="shown.can.retry" variant="primary" :busy="busy" @click="act('retry')">
              {{ t("settings.calibration.wizard.retry") }}
            </Button>
            <Button v-if="shown.can.skip" :busy="busy" @click="act('skip')">
              {{ t("settings.calibration.wizard.skip") }}
            </Button>
            <Button variant="bare" :busy="busy" @click="act('cancel')">
              {{ t("settings.calibration.wizard.cancel") }}
            </Button>
          </div>
        </section>
        <p v-if="shown?.ended" class="said" data-said="wizard-ended">
          {{
            t(`settings.calibration.wizard.ended.${shown.ended}`, {
              settled: shown.settled,
              count: shown.steps.length,
            })
          }}
        </p>
        <div v-if="!shown || shown.ended" class="buttons">
          <Button :busy="busy" @click="act('start')">
            {{
              t(shown ? "settings.calibration.wizard.again" : "settings.calibration.wizard.start")
            }}
          </Button>
        </div>
        <p v-if="refused" class="failed" role="alert">
          {{ t("settings.calibration.refused", { why: refused }) }}
        </p>
      </div>
      <!-- The live image, while the wizard is under way: each step's moves and frames are seen. -->
      <div v-if="shown && !shown.ended" class="live">
        <LiveViewer />
      </div>
    </div>
  </section>
</template>

<style scoped>
.pane,
.steps-and-step,
.step {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.body {
  display: grid;
  gap: var(--space-3);
}

.body.under {
  grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr));
  align-items: start;
}

.steps {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.steps li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
  padding: var(--space-1) var(--space-2);
  border-inline-start: 3px solid transparent;
}

.steps li.current {
  border-inline-start-color: var(--accent);
  background: var(--surface-open);
}

.number {
  inline-size: 1.5em;
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.title {
  font-weight: 600;
}

.found {
  flex-basis: 100%;
  padding-inline-start: 1.5em;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.step {
  padding: var(--space-2);
  border: 1px solid var(--border);
}

.prompt {
  margin: 0;
}

.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.live {
  block-size: 30rem;
  border: 1px solid var(--border);
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.failed {
  margin: 0;
  color: var(--danger);
}
</style>
