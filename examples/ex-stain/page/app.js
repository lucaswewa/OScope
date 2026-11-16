// P45's stain analysis: the newest project's mosaics and each stain's
// image, chosen from a list and browsed in OpenSeadragon, and each
// separation's summary from its `summary.json`.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

let viewer = null;
let project = null;
let shown = "";
const summaries = new Map();

const fixed = (v, n = 3) => (typeof v === "number" ? v.toFixed(n) : "");

// The newest project's results: its images to choose from, and the
// separations' summaries.
async function refresh() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.filter((s) => s.results > 0).at(0);
  if (!found) return;
  project = found.id;
  const contents = await read(`/api/v1/projects/project/${project}`);
  const images = contents.results.filter((r) => r.image && !r.name.endsWith(": stain amounts"));
  const choices = images.map((r) => {
    const option = document.createElement("option");
    option.value = r.id;
    option.textContent = r.name;
    return option;
  });
  const chosen = $("image").value;
  $("image").replaceChildren(...choices);
  if (chosen) $("image").value = chosen;
  for (const r of contents.results.filter((r) => r.name.endsWith(": stain amounts"))) {
    if (!summaries.has(r.id)) {
      const summary = await read(`/api/v1/data/${project}/results/${r.id}/summary.json`).catch(() => null);
      if (summary) summaries.set(r.id, { name: r.name.replace(": stain amounts", ""), summary });
    }
  }
  table();
  if ($("image").value && $("image").value !== shown) show($("image").value);
}

function table() {
  const rows = [...summaries.values()].map(({ name, summary: s }) => {
    const row = document.createElement("tr");
    const dab = s.dab;
    const cells = [
      name,
      s.names.slice(0, 2).join(" and ") + (s.estimated ? " (Macenko)" : ""),
      s.white.map((v) => fixed(v)).join(", "),
      `${fixed(s.means[0])}, ${fixed(s.means[1])}`,
      fixed(s.residual_rms, 4),
      dab ? `${fixed(dab.positive_percent, 1)} %` : "",
      dab ? fixed(dab.h_score, 0) : "",
    ];
    for (const text of cells) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    return row;
  });
  $("rows").replaceChildren(...rows);
}

async function show(id) {
  shown = id;
  const zoom = `/api/v1/zoom/${project}/results/${id}`;
  const status = await read(`${zoom}/status`);
  const source = {
    width: status.size[0],
    height: status.size[1],
    tileSize: 256,
    tileOverlap: 0,
    getTileUrl: (level, x, y) => `${zoom}/image_files/${level}/${x}_${y}.jpg`,
  };
  if (!viewer) {
    viewer = OpenSeadragon({
      element: $("viewer"),
      tileSources: source,
      showNavigationControl: false,
      showNavigator: true,
      navigatorPosition: "BOTTOM_RIGHT",
    });
  } else {
    viewer.addTiledImage({ tileSource: source, index: 0, replace: true });
  }
  $("message").textContent = `: ${$("image").selectedOptions[0]?.textContent ?? ""}`;
}

$("image").addEventListener("change", () => show($("image").value));
const tick = () => refresh().catch((error) => ($("message").textContent = `: ${error.message}`));
tick();
setInterval(tick, 3000);
