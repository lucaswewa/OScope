// Checkpoint F's report: the newest project's reports, each with its PDF and
// HTML, the newest one's HTML shown below.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

async function start() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.filter((s) => s.reports > 0).at(0);
  if (!found) {
    $("message").textContent = ": waiting for a report…";
    setTimeout(start, 1000);
    return;
  }
  const contents = await read(`/api/v1/projects/project/${found.id}`);
  const items = contents.reports.map((r) => {
    const item = document.createElement("li");
    item.append(`${r.name}, ${new Date(r.created).toLocaleTimeString()}: `);
    for (const file of r.files.filter((f) => f !== "model.json")) {
      const link = document.createElement("a");
      link.href = `/api/v1/data/${found.id}/reports/${r.id}/${file}`;
      link.textContent = file;
      link.target = "_blank";
      item.append(link, " ");
    }
    return item;
  });
  $("reports").replaceChildren(...items);
  const shown = contents.reports.findLast((r) => r.files.includes("report.html"));
  if (shown) $("view").src = `/api/v1/data/${found.id}/reports/${shown.id}/report.html`;
  $("message").textContent = `: ${contents.reports.length} of ${found.name}`;
}

start().catch((error) => ($("message").textContent = `: ${error.message}`));
