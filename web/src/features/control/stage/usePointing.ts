// Pointing at the live image (W3.3): a double click brings the place to the
// view's centre, and a drag pans the stage, through the active geometry
// calibration's matrix (P31). Without one, it says what is missing: an
// objective fitted, or the geometry of the one fitted at the camera's
// binning, as the server keeps one for each.
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import { geometryOf, moveFor, plus, type Xyz } from "@/features/control/stage/stage";
import { useStage } from "@/features/control/stage/useStage";
import { notify } from "@/ui/toasts";

export function usePointing() {
  const { t } = useI18n();
  const stage = useStage();
  const active = useProperty("/api/v1/calibration/active", { every: 5_000 });
  const fitted = useProperty("/api/v1/objective/fitted", { every: 5_000 });
  const mode = useProperty("/api/v1/camera/mode", { every: 5_000 });
  const camera = useProperty("/api/v1/camera/capabilities", { every: 60_000 });

  const geometry = computed(() => geometryOf(active.value.value?.geometry?.values));

  /** What pointing lacks, in words: nothing with a calibration. */
  const missing = computed(() => {
    if (geometry.value) return undefined;
    const objective = fitted.value.value;
    if (objective === null) return t("stage.pointing.noObjective");
    const modes = camera.value.value?.modes ?? [];
    const binning = modes.find((one) => one.name === mode.value.value)?.binning ?? 1;
    const name = objective?.name ?? t("stage.pointing.fitted");
    return t("stage.pointing.noGeometry", { objective: name, binning });
  });

  /** Whether pointing lacks its calibration: said, if it does. */
  function lacking(): boolean {
    if (geometry.value) return false;
    notify({ level: "warning", text: missing.value ?? "" });
    return true;
  }

  /** Moves the stage so that what the image shows at the sensor's point `from` comes to `to`. */
  function bring(from: [number, number], to: [number, number]): void {
    if (!lacking()) void stage.go(moveFor(geometry.value!, from, to));
  }

  /**
   * Where the sensor's point `at` is on the stage, with a calibration: the
   * stage's position is that of the sensor's centre, `centre`.
   */
  function placeOf(at: [number, number], centre: [number, number]): Xyz | undefined {
    const here = stage.here.value;
    return geometry.value && here ? plus(here, moveFor(geometry.value, at, centre)) : undefined;
  }

  return { geometry, missing, lacking, bring, placeOf, unit: stage.unit, stage };
}
