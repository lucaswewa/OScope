// Draws the latest overview, the tissue found in it, and the tissue scan's
// tiles as they arrive. Polls the scan Thing each second.

const canvas = document.getElementById("overview");
const summary = document.getElementById("summary");
const context = canvas.getContext("2d");
let shown = "";
let picture = null;

/// The overview's colours as an image the canvas draws, once a new one comes.
function image(view) {
  const bytes = atob(view.image);
  const pixels = context.createImageData(view.width, view.height);
  for (let i = 0, j = 0; i < bytes.length; i += 3, j += 4) {
    pixels.data[j] = bytes.charCodeAt(i);
    pixels.data[j + 1] = bytes.charCodeAt(i + 1);
    pixels.data[j + 2] = bytes.charCodeAt(i + 2);
    pixels.data[j + 3] = 255;
  }
  return pixels;
}

function draw(view, progress) {
  const scale = Math.min(canvas.width / view.width, canvas.height / view.height);
  const at = (x, y) => [((x - view.origin.x) / view.pixel_um) * scale, ((y - view.origin.y) / view.pixel_um) * scale];
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.save();
  context.scale(scale, scale);
  context.putImageData(picture, 0, 0);
  context.restore();
  context.fillStyle = "rgba(60, 200, 90, 0.35)";
  for (const [row, from, to] of view.tissue) {
    context.fillRect(from * scale, row * scale, (to - from) * scale, scale);
  }
  if (!progress) return;
  const corners = progress.plan.field.corners_um;
  const done = new Set(progress.done.map((t) => t.index));
  for (const tile of progress.plan.tiles) {
    context.beginPath();
    corners.forEach((c, i) => {
      const [x, y] = at(tile.x_um + c.x, tile.y_um + c.y);
      if (i) context.lineTo(x, y); else context.moveTo(x, y);
    });
    context.closePath();
    if (done.has(tile.index)) {
      context.fillStyle = "rgba(240, 240, 255, 0.45)";
      context.fill();
    }
    context.strokeStyle = "#fff";
    context.stroke();
  }
}

async function poll() {
  try {
    const view = await (await fetch("/api/v1/scan/last_overview")).json();
    if (!view) return;
    if (view.image !== shown) {
      picture = image(view);
      shown = view.image;
    }
    const progress = await (await fetch("/api/v1/scan/progress")).json();
    draw(view, progress);
    const tiles = progress ? `; the scan ${progress.state}, ${progress.done.length} of ${progress.plan.tiles.length} tiles` : "";
    summary.textContent = `${view.frames} frames, ${view.width} × ${view.height} pixels of ${view.pixel_um} µm; `
      + `${view.area_mm2.toFixed(1)} mm² of tissue in ${view.kept} pieces, at saturation over `
      + `${view.threshold.toFixed(3)}${tiles}.`;
  } catch (error) {
    summary.textContent = `The server didn't answer: ${error}`;
  }
}

poll();
setInterval(poll, 1000);
