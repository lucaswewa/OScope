// Draws the latest scan's tiles as they arrive: every tile of its plan
// outlined, each done filled by the z it was focused at, and the stage's
// place. Polls the scan's progress and the stage's position twice a second.

const canvas = document.getElementById("tiles");
const summary = document.getElementById("summary");
const context = canvas.getContext("2d");

function draw(progress, stage) {
  const { width, height } = canvas;
  context.clearRect(0, 0, width, height);
  const { plan, done } = progress;
  const corners = plan.field.corners_um;
  const frames = plan.tiles.map((t) => corners.map((c) => [t.x_um + c.x, t.y_um + c.y]));
  const xs = frames.flat().map((p) => p[0]);
  const ys = frames.flat().map((p) => p[1]);
  const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
  const scale = Math.min((width - 20) / (Math.max(...xs) - x0 || 1),
    (height - 20) / (Math.max(...ys) - y0 || 1));
  const at = ([x, y]) => [10 + (x - x0) * scale, 10 + (y - y0) * scale];
  const outline = (frame) => {
    context.beginPath();
    frame.forEach((p, i) => (i ? context.lineTo(...at(p)) : context.moveTo(...at(p))));
    context.closePath();
  };
  const zs = done.map((t) => t.z_um);
  const [low, high] = [Math.min(...zs), Math.max(...zs)];
  const byIndex = new Map(done.map((t) => [t.index, t]));
  plan.tiles.forEach((tile, i) => {
    outline(frames[i]);
    const taken = byIndex.get(tile.index);
    if (taken) {
      const f = (taken.z_um - low) / (high - low || 1);
      context.fillStyle = `hsla(${240 - 240 * f}, 70%, 55%, 0.6)`;
      context.fill();
    }
    context.strokeStyle = "#888";
    context.stroke();
  });
  if (stage) {
    const [sx, sy] = at([stage.x, stage.y]);
    context.fillStyle = "#000";
    context.beginPath();
    context.arc(sx, sy, 5, 0, 2 * Math.PI);
    context.fill();
  }
  const range = done.length ? `; z from ${low.toFixed(2)} to ${high.toFixed(2)} µm` : "";
  summary.textContent = `${done.length} of ${plan.tiles.length} tiles, ${progress.state}`
    + `${range}; ${progress.refused} autofocus runs refused.`;
}

async function poll() {
  try {
    const progress = await (await fetch("/api/v1/scan/progress")).json();
    if (progress) {
      const position = await (await fetch("/api/v1/stage/position")).json();
      draw(progress, position && position.position);
    }
  } catch (error) {
    summary.textContent = `The server didn't answer: ${error}`;
  }
}

poll();
setInterval(poll, 500);
