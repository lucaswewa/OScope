// The newest stitched mosaic (P40), browsed in OpenSeadragon from the deep
// zoom routes: its look set by levels and gamma, each change a look of its
// own whose tiles the server makes and keeps; and the tiles' times, from
// their Server-Timing.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// The newest result of the newest project that has one.
async function newest() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.filter((s) => s.results > 0).at(0);
  if (!found) return null;
  const contents = await read(`/api/v1/projects/project/${found.id}`);
  const result = contents.results.filter((r) => r.image).at(-1);
  return result ? { project: found.id, result } : null;
}

let viewer = null;
let base = "";
let status = null;
let asMade = true;

function query() {
  if (asMade) return "";
  const look = ["black", "white", "gamma"].map((k) => `${k}=${$(k).value}`);
  return `?${look.join("&")}`;
}

function source() {
  const [width, height] = status.size;
  const look = query();
  return {
    width,
    height,
    tileSize: 256,
    tileOverlap: 0,
    getTileUrl: (level, x, y) => `${base}/image_files/${level}/${x}_${y}.jpg${look}`,
  };
}

function show() {
  for (const k of ["black", "white", "gamma"]) $(`${k}-value`).textContent = Number($(k).value).toFixed(2);
  if (!viewer) {
    viewer = OpenSeadragon({
      element: $("viewer"),
      tileSources: source(),
      showNavigationControl: false,
      showNavigator: true,
      navigatorPosition: "BOTTOM_RIGHT",
    });
    return;
  }
  // The new look in place of the old, where the view is.
  viewer.addTiledImage({ tileSource: source(), index: 0, replace: true });
}

function lookOf(made) {
  $("black").value = made.black;
  $("white").value = made.white;
  // sRGB's curve is close to a gamma of 2.2.
  $("gamma").value = made.gamma ?? 2.2;
}

// The tiles' server times, from Server-Timing, hits and misses apart.
const times = { hit: [], miss: [] };
const median = (v) => (v.length ? [...v].sort((a, b) => a - b)[v.length >> 1] : NaN);
new PerformanceObserver((list) => {
  for (const entry of list.getEntries()) {
    if (!entry.name.includes("/image_files/")) continue;
    for (const t of entry.serverTiming ?? []) {
      if (t.name === "tile" && times[t.description]) times[t.description].push(t.duration);
    }
  }
  const line = (k, label) =>
    `${times[k].length} ${label}, ${median(times[k]).toFixed(2)} ms of server time each, the median`;
  $("times").textContent = `Tiles: ${line("miss", "made")}; ${line("hit", "kept")}.`;
}).observe({ type: "resource", buffered: true });

async function start() {
  const found = await newest();
  if (!found) {
    $("message").textContent = ": no stitched mosaic yet; the example scans and stitches as it starts.";
    setTimeout(start, 3000);
    return;
  }
  const { project, result } = found;
  base = `/api/v1/zoom/${project}/results/${result.id}`;
  status = await read(`${base}/status`);
  $("message").textContent = `: ${result.name}, ${status.size[0]} × ${status.size[1]} pixels, ${status.levels} levels`;
  lookOf(status.look);
  for (const k of ["black", "white", "gamma"]) {
    $(k).addEventListener("input", () => ($(`${k}-value`).textContent = Number($(k).value).toFixed(2)));
    $(k).addEventListener("change", () => {
      asMade = false;
      show();
    });
  }
  $("reset").addEventListener("click", () => {
    asMade = true;
    lookOf(status.look);
    show();
  });
  show();
}

start().catch((error) => {
  $("message").textContent = `: ${error.message}`;
  $("message").className = "bad";
});
