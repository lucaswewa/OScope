// Each job's end, told (W4.1): in the app always, as a notice with the way to
// the job; and as the system's notification too when the app can't be seen,
// through the host adapter (ADR-0046), as the owner chose.
import { watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import { JOB_ENDED, type Job, type JobState } from "@/api/jobs";
import { useEvent } from "@/api/things/use";
import { host } from "@/host/host";
import { notify, type Level } from "@/ui/toasts";

/** How much each end matters. */
const LEVELS: Partial<Record<JobState, Level>> = {
  completed: "success",
  failed: "danger",
  interrupted: "warning",
  cancelled: "info",
};

/** Tells each job's end, from the jobs Thing's `changed` events: for the shell, once a page. */
export function useJobEnds(): void {
  const { t } = useI18n();
  const router = useRouter();
  const changed = useEvent<Job>("/api/v1/jobs/changed");
  // Each event read once, and each job's end told once.
  const read = new WeakSet<object>();
  const told = new Set<string>();
  watch(
    () => changed.told.value,
    (events) => {
      for (const event of events) {
        if (read.has(event)) continue;
        read.add(event);
        const job = event.data;
        if (!JOB_ENDED.has(job.state) || told.has(job.id)) continue;
        told.add(job.id);
        const text = t(`jobs.ended.${job.state}`, { title: job.title });
        notify({
          level: LEVELS[job.state] ?? "info",
          text,
          action: {
            label: t("jobs.ended.show"),
            run: () => void router.push({ name: "jobs", query: { job: job.id } }),
          },
        });
        if (host().window.isHidden()) void host().notify(t("jobs.ended.title"), text);
      }
    },
  );
}
