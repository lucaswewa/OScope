// Draws the latest autofocus run's curves: each frame's measure against the
// stage's z, the coarse (or continuous) sweep grey, the fine sweep red, and
// where the stage was left blue. Polls the report each second.

const canvas = document.getElementById("curve");
const summary = document.getElementById("summary");
const context = canvas.getContext("2d");
let shown = "";

function draw(report) {
  const points = report.coarse.concat(report.fine);
  const { width, height } = canvas;
  const margin = 48;
  context.clearRect(0, 0, width, height);
  if (points.length === 0) return;
  const zs = points.map((p) => p.z_um).concat([report.z_um]);
  const values = points.map((p) => p.value);
  const [zLow, zHigh] = [Math.min(...zs), Math.max(...zs)];
  const top = Math.max(...values) || 1;
  const x = (z) => margin + ((z - zLow) / (zHigh - zLow || 1)) * (width - 2 * margin);
  const y = (v) => height - margin - (v / top) * (height - 2 * margin);
  context.strokeStyle = "#999";
  context.beginPath();
  context.moveTo(margin, margin);
  context.lineTo(margin, height - margin);
  context.lineTo(width - margin, height - margin);
  context.stroke();
  context.fillStyle = "#333";
  context.font = "14px system-ui, sans-serif";
  context.fillText(`${zLow.toFixed(1)} µm`, margin, height - margin + 20);
  context.fillText(`${zHigh.toFixed(1)} µm`, width - margin - 70, height - margin + 20);
  context.fillText(report.measure, margin + 6, margin - 10);
  const curve = (pts, colour) => {
    context.strokeStyle = colour;
    context.fillStyle = colour;
    context.beginPath();
    pts.forEach((p, i) => (i ? context.lineTo(x(p.z_um), y(p.value)) : context.moveTo(x(p.z_um), y(p.value))));
    context.stroke();
    pts.forEach((p) => context.fillRect(x(p.z_um) - 2, y(p.value) - 2, 5, 5));
  };
  curve(report.coarse, "#7a7a7a");
  curve(report.fine, "#c0392b");
  context.strokeStyle = "#2471a3";
  context.beginPath();
  context.moveTo(x(report.z_um), margin);
  context.lineTo(x(report.z_um), height - margin);
  context.stroke();
}

async function poll() {
  try {
    const response = await fetch("/api/v1/autofocus/last");
    const report = await response.json();
    const key = JSON.stringify(report);
    if (report && key !== shown) {
      shown = key;
      summary.textContent = report.found
        ? `Focused at z = ${report.z_um.toFixed(3)} µm by a ${report.sweep} sweep of ${report.frames} frames, in ${(report.took_ms / 1000).toFixed(1)} s; the depth of field is ${report.depth_of_field_um.toFixed(2)} µm.`
        : `No focus: ${report.refused}`;
      draw(report);
    }
  } catch (error) {
    summary.textContent = `The server didn't answer: ${error}`;
  }
  setTimeout(poll, 1000);
}

poll();
