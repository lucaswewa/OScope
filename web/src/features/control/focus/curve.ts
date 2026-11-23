// Autofocus's curve (W3.7), from its last run's report (P34): the measure at
// each z of the coarse sweep, and of the fine one, and where the run left
// the stage, each z in µm from where it started.
import type { ValueOf } from "@/api/things/use";
import type { Series } from "@/ui/charts/draw";

/** The last run's report, once there is one. */
export type FocusReport = NonNullable<ValueOf<"/api/v1/autofocus/last">>;

/** What the curve's series are called. */
export interface CurveNames {
  coarse: string;
  fine: string;
  focus: string;
}

/** A sweep's points as a series, in order of z, each from where the run started. */
function sweepOf(
  points: FocusReport["coarse"],
  start: number,
  name: string,
  kind: Series["kind"],
): Series {
  const ordered = [...points].sort((a, b) => a.z_um - b.z_um);
  return { name, kind, x: ordered.map((p) => p.z_um - start), y: ordered.map((p) => p.value) };
}

/**
 * The curve's series: the coarse sweep as a line, the fine one's points, and
 * the focus found as an upright line across the measures, if it found one.
 */
export function curveOf(report: FocusReport, names: CurveNames): Series[] {
  const start = report.start_um;
  const series = [sweepOf(report.coarse, start, names.coarse, "line")];
  if (report.fine.length > 0) series.push(sweepOf(report.fine, start, names.fine, "points"));
  const values = [...report.coarse, ...report.fine].map((p) => p.value);
  if (report.found && values.length > 0) {
    const at = report.z_um - start;
    series.push({
      name: names.focus,
      kind: "line",
      x: [at, at],
      y: [Math.min(...values), Math.max(...values)],
    });
  }
  return series;
}

/** The highest point measured, in either sweep: where the frame was sharpest. */
export function peakOf(report: FocusReport): { z_um: number; value: number } | undefined {
  const points = [...report.coarse, ...report.fine];
  return points.reduce<(typeof points)[number] | undefined>(
    (best, p) => (best === undefined || p.value > best.value ? p : best),
    undefined,
  );
}
