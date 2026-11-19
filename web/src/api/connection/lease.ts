// The control lease (W1.3, ADR-0031): who may change the instrument. One
// account holds it at a time; the others read and watch, and may ask for
// it. `useCanControl` is what every control asks: without accounts,
// everyone may.
import { computed, effectScope, onScopeDispose, shallowRef, watch, type EffectScope } from "vue";

import { activity } from "../client.ts";
import { thingStore } from "../things/store.ts";
import { useAction, useProperty } from "../things/use.ts";
import { useSession } from "./session.ts";

/** How long before the lease lapses its holder is warned: a minute. */
export const WARN_BEFORE = 60_000;

/** Who the caller is, as the `auth` Thing tells it. */
interface Me {
  name: string | null;
  /** Whether the caller holds the lease. */
  control: boolean;
}

/** The lease, and what can be done with it, for as long as the component lasts. */
export function useLease() {
  const lease = useProperty("/api/v1/auth/lease", { every: 2_000 });
  const me = useProperty("/api/v1/auth/me", { every: 2_000 });
  const accounts = useProperty("/api/v1/auth/accounts", { every: 30_000 });
  const holding = computed(() => (me.value.value as Me | undefined)?.control === true);
  // When it became this app's, on the page's clock.
  const since = shallowRef(0);
  watch(holding, (now) => (since.value = now ? performance.now() : 0), { immediate: true });
  const clock = thingStore("auth");
  /** How long, in ms, until the lease lapses unless this app changes something. */
  const lapsesIn = computed(() => {
    const state = lease.value.value;
    if (!holding.value || !state) return undefined;
    const from = Math.max(since.value, activity.last);
    return Math.max(0, state.idle_s * 1000 - (clock.now - from));
  });
  return {
    lease,
    me: computed(() => me.value.value as Me | undefined),
    accounts,
    holding,
    lapsesIn,
    lapsing: computed(() => lapsesIn.value !== undefined && lapsesIn.value < WARN_BEFORE),
    acquire: useAction("/api/v1/auth/acquire_control"),
    request: useAction("/api/v1/auth/request_control"),
    handOver: useAction("/api/v1/auth/hand_over_control"),
    release: useAction("/api/v1/auth/release_control"),
  };
}

/**
 * Whether this app may change the instrument now: always without
 * accounts; with them, while it holds the lease.
 */
export function useCanControl() {
  const session = useSession();
  const holding = shallowRef(false);
  let following: EffectScope | undefined;
  watch(
    () => session.state,
    (state) => {
      following?.stop();
      following = undefined;
      holding.value = false;
      // Only a session has a lease to hold: without accounts there is no `auth` Thing.
      if (state !== "in") return;
      following = effectScope();
      following.run(() => {
        const me = useProperty("/api/v1/auth/me", { every: 2_000 });
        watch(
          () => (me.value.value as Me | undefined)?.control,
          (control) => (holding.value = control === true),
          { immediate: true },
        );
      });
    },
    { immediate: true },
  );
  onScopeDispose(() => following?.stop());
  return computed(() => session.state === "open" || holding.value);
}
