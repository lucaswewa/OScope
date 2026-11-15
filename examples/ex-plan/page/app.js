// Draws the scan plans ex-plan wrote beside this page, in plans.json: the
// region, each tile's frame coloured by its place in the order (or by its
// z, for a plan by a focus map), and the path the stage takes.

const canvas = document.getElementById("plan");
const choice = document.getElementById("choice");
const summary = document.getElementById("summary");
const map = document.getElementById("map");
const context = canvas.getContext("2d");

function duration(seconds) {
  const s = Math.round(seconds);
  const [h, m] = [Math.floor(s / 3600), Math.floor(s / 60) % 60];
  return h ? `${h} h ${m} min` : m ? `${m} min ${s % 60} s` : `${s} s`;
}

function planes(plan) {
  const stack = plan.stack;
  return stack ? Math.floor((stack.below_um + stack.above_um) / stack.step_um + 1e-9) + 1 : 1;
}

function draw({ plan, region, estimate }) {
  const corners = plan.field.corners_um;
  const frames = plan.tiles.map((t) => corners.map((c) => [t.x_um + c.x, t.y_um + c.y]));
  const cell = region.cell_um;
  const shape = region.corners
    || region.runs.flatMap(([row, from, to]) => [from, to].map((i) =>
      [region.origin[0] + i * cell, region.origin[1] + (row + 0.5) * cell]));
  const points = frames.flat().concat(shape);
  const [xs, ys] = [points.map((p) => p[0]), points.map((p) => p[1])];
  const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
  const { width, height } = canvas;
  const scale = Math.min((width - 20) / (Math.max(...xs) - x0 || 1),
    (height - 20) / (Math.max(...ys) - y0 || 1));
  const at = ([x, y]) => [10 + (x - x0) * scale, 10 + (y - y0) * scale];
  const path = (list, close) => {
    context.beginPath();
    list.forEach((p, i) => (i ? context.lineTo(...at(p)) : context.moveTo(...at(p))));
    if (close) context.closePath();
  };
  context.clearRect(0, 0, width, height);

  context.fillStyle = "#dcefd5";
  if (region.corners) {
    path(region.corners, true);
    context.fill();
  } else {
    for (const [row, from, to] of region.runs) {
      const [x, y] = at([region.origin[0] + from * cell, region.origin[1] + row * cell]);
      context.fillRect(x, y, (to - from) * cell * scale, cell * scale + 0.5);
    }
  }

  const mapped = plan.z.kind === "map";
  const zs = plan.tiles.map((t) => t.z_um).filter((z) => z !== null);
  const [zLow, zHigh] = [Math.min(...zs), Math.max(...zs)];
  context.lineWidth = 1;
  plan.tiles.forEach((t, i) => {
    const f = mapped ? (t.z_um - zLow) / (zHigh - zLow || 1) : i / Math.max(1, plan.tiles.length - 1);
    context.strokeStyle = `hsl(${240 - 240 * f}, 70%, 45%)`;
    path(frames[i], true);
    context.stroke();
  });
  context.strokeStyle = "rgba(0, 0, 0, 0.45)";
  path(plan.tiles.map((t) => [t.x_um, t.y_um]), false);
  context.stroke();
  const [sx, sy] = at([plan.tiles[0].x_um, plan.tiles[0].y_um]);
  context.fillStyle = "#000";
  context.beginPath();
  context.arc(sx, sy, 4, 0, 2 * Math.PI);
  context.fill();

  const colour = mapped
    ? `each frame coloured by its z, from ${zLow.toFixed(1)} µm in blue to ${zHigh.toFixed(1)} in red`
    : "each frame coloured by its place, from blue to red";
  const some = (n, what) => `${n} ${what}${n === 1 ? "" : "s"}`;
  summary.textContent = `${some(estimate.tiles, "tile")}, ${plan.columns} × ${plan.rows}, `
    + `${some(planes(plan), "plane")} each: ${some(estimate.frames, "frame")}, `
    + `${(estimate.bytes / 1e9).toFixed(1)} GB, `
    + `${(estimate.travel_um / 1e3).toFixed(1)} mm of travel, about ${duration(estimate.seconds)}; `
    + `${colour}.`;
}

fetch("plans.json")
  .then((answer) => answer.json())
  .then(({ plans, map: measured }) => {
    plans.forEach((p, i) => choice.add(new Option(p.name, String(i))));
    choice.addEventListener("change", () => draw(plans[Number(choice.value)]));
    draw(plans[0]);
    const found = measured.samples.filter((s) => s.z_um !== null).length;
    const [c, tilt] = [measured.plane.coefficients, measured.tilt];
    map.textContent = `The focus map: ${found} of ${measured.samples.length} points focused. `
      + `Its plane rises ${c[1].toFixed(2)} µm a mm along x and ${c[2].toFixed(2)} along y, `
      + `where the simulated slide tilts ${tilt[0]} and ${tilt[1]}.`;
  })
  .catch((error) => {
    summary.textContent = `No plans: ${error}`;
  });
