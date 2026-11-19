// Another's request for the control lease, raised as a notice that hands
// over (W2.2): once for each account that asks, while this app holds it,
// and taken back once the request has gone.
import { watch } from "vue";
import { useI18n } from "vue-i18n";

import { useLease } from "@/api/connection/lease";
import { notify, toasts } from "@/ui/toasts";

/**
 * Given the askers told of already, which it keeps, those to tell of now
 * and those whose request has gone: each asker is told of once, and again
 * only after it stopped asking. While this app doesn't hold the lease,
 * every request has gone.
 */
export function requestsToTell(
  told: Set<string>,
  holding: boolean,
  requests: readonly string[],
): { fresh: string[]; gone: string[] } {
  const asking = holding ? requests : [];
  const gone = [...told].filter((name) => !asking.includes(name));
  gone.forEach((name) => told.delete(name));
  const fresh = asking.filter((name) => !told.has(name));
  fresh.forEach((name) => told.add(name));
  return { fresh, gone };
}

/** Raises a notice for each new request, with the means to hand over: for the shell. */
export function useLeaseRequests(): void {
  const { t } = useI18n();
  const lease = useLease();
  const told = new Set<string>();
  const notices = new Map<string, number>();
  watch(
    () => [lease.holding.value, lease.lease.value.value?.requests ?? []] as const,
    ([holding, requests]) => {
      const { fresh, gone } = requestsToTell(told, holding, requests);
      for (const name of gone) {
        toasts.dismiss(notices.get(name) ?? 0);
        notices.delete(name);
      }
      for (const name of fresh) {
        const id = notify({
          level: "info",
          text: t("status.lease.asked", { name }),
          lasts: 0,
          action: {
            label: t("status.lease.handOver"),
            run: () => void lease.handOver.run({ to: name }),
          },
        });
        notices.set(name, id);
      }
    },
    { immediate: true },
  );
}
