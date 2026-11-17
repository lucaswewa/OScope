// P46's cell count on the newest stitched mosaic: the mosaic in
// OpenSeadragon, the nuclei's outlines over it from its annotation layer
// `cells`, and the count's summary.
"use strict";

const $ = (id) => document.getElementById(id);
const SVG = "http://www.w3.org/2000/svg";

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

let viewer = null;
let pixel = [1, 1];
let outlines = [];

function draw() {
  const overlay = $("overlay");
  overlay.replaceChildren();
  if (!viewer || !$("show").checked) return;
  const screen = ([x, y]) => {
    const p = viewer.viewport.imageToViewerElementCoordinates(new OpenSeadragon.Point(x / pixel[0], y / pixel[1]));
    return `${p.x},${p.y}`;
  };
  for (const f of outlines) {
    const path = document.createElementNS(SVG, "path");
    path.setAttribute("d", `M${f.geometry.coordinates[0].map(screen).join(" ")}Z`);
    path.setAttribute("class", f.properties.class ?? "cell");
    overlay.append(path);
  }
}

async function start() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.filter((s) => s.results > 1).at(0);
  const contents = found && (await read(`/api/v1/projects/project/${found.id}`));
  const mosaic = contents?.results.filter((r) => r.image).at(-1);
  const counted = contents?.results.filter((r) => r.name.endsWith(": cells")).at(-1);
  if (!mosaic || !counted) {
    $("message").textContent = ": waiting for a counted mosaic…";
    setTimeout(start, 1000);
    return;
  }
  const size = mosaic.image.pixel_size;
  pixel = size ? [size.x, size.y] : [1, 1];
  const record = `${found.id}/results/${mosaic.id}`;
  const layers = await read(`/api/v1/workspace/annotations/${record}`);
  outlines = layers.layers.cells?.features ?? [];
  const s = await read(`/api/v1/data/${found.id}/results/${counted.id}/summary.json`);
  $("message").textContent = `: ${s.count} nuclei, ${s.per_mm2.toFixed(0)} per mm² of tissue, of ${s.mean_area.toFixed(1)} µm² and circularity ${s.mean_circularity.toFixed(2)} on average`;
  const zoom = `/api/v1/zoom/${record}`;
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
  for (const name of ["animation", "update-viewport", "resize", "open"]) viewer.addHandler(name, draw);
  $("show").addEventListener("change", draw);
  $("csv").href = `/api/v1/data/${found.id}/results/${counted.id}/cells.csv`;
}

start().catch((error) => ($("message").textContent = `: ${error.message}`));
