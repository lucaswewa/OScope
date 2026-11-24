// The stage, live (W3.3): where it is and what it is doing, and moving it,
// for its pane, the focused image and Halt. A move past the limits is
// refused before it is sent; a jog is stopped when its key or button is let
// go, and when the page loses the keys or is left.
import { computed, onScopeDispose } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import {
  AXES,
  beyond,
  movedBy,
  plus,
  stepBy,
  within,
  written,
  type Axis,
  type Step,
  type Unit,
  type Xyz,
} from "@/features/control/stage/stage";
import { notify } from "@/ui/toasts";
import { useAccountDisplay } from "@/features/display/account";

export function useStage() {
  const { t } = useI18n();
  const capabilities = useProperty("/api/v1/stage/capabilities", { every: 60_000 });
  // Read often while it moves, and again at each of its events: `moved` at rest.
  const position = useProperty("/api/v1/stage/position", { every: 500 });
  const status = useProperty("/api/v1/stage/status", { every: 1_000 });
  // The account's units, as the Settings tab changes them, here or in another browser (W8.1).
  const account = useAccountDisplay();
  const moveTo = useAction("/api/v1/stage/move_to");
  const moveBy = useAction("/api/v1/stage/move_by");
  const jogging = useAction("/api/v1/stage/jog");
  const halting = useAction("/api/v1/stage/halt");

  const unit = computed<Unit>(() => account.display?.units ?? "um");
  const here = computed(() => position.value.value?.position);

  /** Whether `target` is past the limits along `axes`, said if it is: nothing is then sent. */
  function refused(target: Xyz, axes: readonly Axis[] = AXES): boolean {
    const limits = capabilities.value.value;
    if (!limits) return true;
    const past = beyond(target, limits, axes);
    for (const axis of past) {
      const { min, max } = limits[axis];
      notify({
        level: "warning",
        text: t("stage.beyond", {
          axis: axis.toUpperCase(),
          min: written(min, unit.value),
          max: written(max, unit.value),
          unit: t(`stage.units.${unit.value}`),
        }),
      });
    }
    return past.length > 0;
  }

  /** Moves to `target`, in µm, unless it is past the limits. */
  async function goTo(target: Xyz): Promise<void> {
    if (!refused(target)) await moveTo.run(target);
  }

  /** Moves by `by`, in µm, unless that ends past the limits along the axes it moves. */
  async function go(by: Partial<Xyz>): Promise<void> {
    const from = here.value;
    const limits = capabilities.value.value;
    const moved = movedBy(by);
    if (!from || !limits || refused(plus(from, by), moved)) return;
    // The server moves by from where the stage says it is, which homing leaves a
    // micrometre past its limits: from there, the stage is sent where to go,
    // each axis it leaves alone brought within them.
    if (beyond(from, limits).length > 0) {
      const target = plus(from, by);
      for (const axis of AXES) {
        if (!moved.includes(axis)) target[axis] = within(from[axis], limits[axis]);
      }
      await moveTo.run(target);
      return;
    }
    await moveBy.run({ dx: by.x ?? 0, dy: by.y ?? 0, dz: by.z ?? 0 });
  }

  /** A step of `size` µm, unless one is under way: a held key's repeats don't pile up. */
  async function step(towards: Step, size: number): Promise<void> {
    if (!moveBy.running.value && !moveTo.running.value) await go(stepBy(towards, size));
  }

  /** Stops the stage: a move, a jog, homing. */
  async function halt(): Promise<void> {
    await halting.run({});
  }

  // A jog runs until halted. Its halt waits for its start, which a quick
  // press could otherwise overtake, and the stage would never stop.
  let started: Promise<unknown> | undefined;
  /** Starts jogging, until `endJog`. */
  function jog(towards: Step): void {
    if (started) return;
    started = jogging.run({ axis: towards.axis, towards_max: towards.towardsMax });
  }
  /** Stops a jog this page started, if one is under way. */
  async function endJog(): Promise<void> {
    if (!started) return;
    const was = started;
    started = undefined;
    await was;
    await halt();
  }
  const lost = () => void endJog();
  globalThis.addEventListener("blur", lost);
  onScopeDispose(() => {
    globalThis.removeEventListener("blur", lost);
    lost();
  });

  return {
    capabilities,
    position,
    status,
    unit,
    here,
    goTo,
    go,
    step,
    jog,
    endJog,
    halt,
    moveTo,
  };
}
