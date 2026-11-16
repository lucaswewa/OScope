// The newest mosaic stitched in the ex-stitch project (P39): its thumbnail,
// and its stitch.json's numbers, read through the projects Thing and the
// data route.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

async function show() {
  const page = await read("/api/v1/projects/list?text=ex-stitch");
  const found = page.projects.find((s) => s.project.name === "ex-stitch" && s.results > 0);
  if (!found) {
    $("message").textContent = "No stitch yet: the example scans and stitches as it starts.";
    return;
  }
  const project = found.project.id;
  const contents = await read(`/api/v1/projects/project/${project}`);
  const result = contents.results.at(-1);
  const data = `/api/v1/data/${project}/results/${result.id}`;
  const report = await read(`${data}/stitch.json`);
  $("message").textContent = `${result.name}, made ${new Date(result.created).toLocaleString()}.`;
  const rows = [
    ["Tiles", report.tiles],
    ["Pairs kept", `${report.kept} of ${report.pairs}; ${report.dropped} too unsure, ${report.rejected} rejected`],
    ["Missed by", `${report.rms_px.toFixed(2)} px rms`],
    ["Moved from the stage", `${report.moved_px.toFixed(2)} px rms`],
    ["Size", `${report.size[0]} × ${report.size[1]} pixels`],
  ];
  $("report").replaceChildren(
    ...rows.flatMap(([name, value]) => {
      const dt = document.createElement("dt");
      const dd = document.createElement("dd");
      dt.textContent = name;
      dd.textContent = value;
      return [dt, dd];
    }),
  );
  $("mosaic").src = `${data}/thumbnail.jpg`;
}

show().catch((error) => {
  $("message").textContent = error.message;
  $("message").className = "bad";
});
