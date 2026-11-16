// P44's annotations on the newest stitched mosaic: drawn over OpenSeadragon
// in µm, kept in layers through the `projects` Thing, and measured there:
// lengths, areas, and their pixels' statistics, with the CSV to download.
"use strict";

const $ = (id) => document.getElementById(id);
const SVG = "http://www.w3.org/2000/svg";

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "";
}

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// The newest result with an image, of the newest project that has one.
async function newest() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.filter((s) => s.results > 0).at(0);
  if (!found) return null;
  const contents = await read(`/api/v1/projects/project/${found.id}`);
  const result = contents.results.filter((r) => r.image).at(-1);
  return result ? { project: found.id, result } : null;
}

// Invokes the `projects` Thing's `action`, and waits for its output.
async function act(action, input) {
  const answer = await fetch(`/api/v1/projects/${action}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  if (answer.status !== 201) throw new Error(`${action}: ${await answer.text()}`);
  const { href } = await answer.json();
  for (;;) {
    const invocation = await read(href);
    if (invocation.status === "completed") return invocation.output;
    if (!["pending", "running"].includes(invocation.status)) {
      throw new Error(`${action}: ${JSON.stringify(invocation.error ?? invocation)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

let viewer = null;
let target = null; // { project, result, pixel: [x, y] in µm }
let annotations = { layers: {} };
let drawing = []; // the shape being drawn, in µm

const record = () => `${target.project}/results/${target.result}`;

// µm to the viewer's pixels, and the viewer's pixels to µm.
function onScreen([x, y]) {
  const at = new OpenSeadragon.Point(x / target.pixel[0], y / target.pixel[1]);
  const p = viewer.viewport.imageToViewerElementCoordinates(at);
  return [p.x, p.y];
}
function inMicrometres(position) {
  const p = viewer.viewport.viewerElementToImageCoordinates(position);
  return [p.x * target.pixel[0], p.y * target.pixel[1]];
}

function draw() {
  const overlay = $("overlay");
  overlay.replaceChildren();
  const add = (tag, attributes, text) => {
    const shape = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attributes)) shape.setAttribute(k, v);
    if (text) shape.textContent = text;
    overlay.append(shape);
  };
  const points = (list) => list.map((p) => onScreen(p).join(",")).join(" ");
  for (const [layer, collection] of Object.entries(annotations.layers)) {
    for (const f of collection.features) {
      const g = f.geometry;
      const first = g.type === "Point" ? g.coordinates : g.type === "LineString" ? g.coordinates[0] : g.coordinates[0][0];
      if (g.type === "Polygon") add("path", { d: g.coordinates.map((r) => `M${points(r)}Z`).join(" "), class: "area" });
      if (g.type === "LineString") add("polyline", { points: points(g.coordinates), class: "line" });
      const [x, y] = onScreen(first);
      if (g.type === "Point") add("circle", { cx: x, cy: y, r: 5, class: "point" });
      add("text", { x: x + 6, y: y - 6 }, `${layer} ${f.id}`);
    }
  }
  if (drawing.length) add("polyline", { points: points(drawing), class: "drawing" });
}

function table(measured) {
  const fixed = (v) => (Number.isFinite(v) ? v.toFixed(1) : "");
  const rows = measured.map((m) => {
    const row = document.createElement("tr");
    const pixels = m.channels.reduce((n, c) => n + c.pixels, 0);
    const means = m.channels.map((c) => `${c.name} ${fixed(c.mean)} ± ${fixed(c.sd)}`).join("; ");
    const cells = [m.layer, m.id, m.name ?? "", m.geometry, fixed(m.length), fixed(m.area), pixels || "", means];
    for (const text of cells) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    const remove = document.createElement("button");
    remove.textContent = "Remove";
    remove.onclick = () => removed(m.layer, m.id);
    row.append(remove);
    return row;
  });
  $("rows").replaceChildren(...rows);
  const unit = measured[0]?.unit ?? "µm";
  $("unit").textContent = unit;
  $("area-unit").textContent = `${unit}²`;
}

async function refresh() {
  annotations = await read(`/api/v1/projects/annotations/${record()}`);
  table(await read(`/api/v1/projects/measurements/${record()}`));
  draw();
}

async function kept(geometry) {
  const name = $("name").value.trim();
  const feature = { type: "Feature", geometry, properties: name ? { name } : {} };
  const layer = $("layer").value.trim() || "regions";
  try {
    const made = await act("annotate", { project: target.project, kind: "result", record: target.result, layer, feature });
    say(`kept ${geometry.type} ${made.id} in ${layer}`);
    await refresh();
  } catch (error) {
    say(error.message, true);
  }
}

async function removed(layer, feature) {
  try {
    await act("remove_annotation", { project: target.project, kind: "result", record: target.result, layer, feature });
    await refresh();
  } catch (error) {
    say(error.message, true);
  }
}

// Ends the shape being drawn: a polygon of three corners or more, closed,
// or a line of two points or more.
function finish() {
  const shape = $("shape").value;
  const enough = shape === "Polygon" ? 3 : 2;
  const points = drawing;
  drawing = [];
  if (points.length < enough) return draw();
  const coordinates = shape === "Polygon" ? [[...points, points[0]]] : points;
  kept({ type: shape, coordinates });
}

async function start() {
  const found = await newest().catch(() => null);
  if (!found) {
    say(": waiting for a stitched mosaic…");
    setTimeout(start, 1000);
    return;
  }
  const size = found.result.image.pixel_size;
  target = { project: found.project, result: found.result.id, pixel: size ? [size.x, size.y] : [1, 1] };
  const zoom = `/api/v1/zoom/${record()}`;
  const status = await read(`${zoom}/status`);
  viewer = OpenSeadragon({
    element: $("viewer"),
    tileSources: {
      width: status.size[0],
      height: status.size[1],
      tileSize: 256,
      tileOverlap: 0,
      getTileUrl: (level, x, y) => `${zoom}/image_files/${level}/${x}_${y}.jpg`,
    },
    showNavigationControl: false,
    showNavigator: true,
    navigatorPosition: "BOTTOM_RIGHT",
  });
  viewer.addHandler("canvas-click", (event) => {
    if (!$("draw").checked || !event.quick) return;
    event.preventDefaultAction = true;
    const at = inMicrometres(event.position);
    if ($("shape").value === "Point") return kept({ type: "Point", coordinates: at });
    drawing.push(at);
    draw();
  });
  viewer.addHandler("canvas-double-click", (event) => {
    if (!$("draw").checked) return;
    event.preventDefaultAction = true;
    // Its second click added a point where the first did.
    drawing.pop();
    finish();
  });
  for (const name of ["animation", "update-viewport", "resize", "open"]) viewer.addHandler(name, draw);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      drawing = [];
      draw();
    }
    if (event.key === "Enter") finish();
  });
  $("csv").href = `/api/v1/projects/measurements/${record()}?format=csv`;
  say(`: ${status.size[0]} × ${status.size[1]} pixels of ${target.pixel[0].toFixed(3)} µm`);
  await refresh();
}

start().catch((error) => say(error.message, true));
