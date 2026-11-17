// P48's Workspace: the newest project's results, each with its provenance
// and files, and what differs from the first analysis.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

function cell(row, text) {
  const td = document.createElement("td");
  td.textContent = text;
  row.append(td);
  return td;
}

async function start() {
  const page = await read("/api/v1/projects/list");
  const found = page.projects.at(0);
  const results = found ? await read(`/api/v1/workspace/results/${found.id}`) : [];
  const analyses = results.filter((r) => r.provenance.recipe && r.provenance.recipe["workspace.run"]);
  if (analyses.length === 0) {
    $("message").textContent = ": waiting for an analysis…";
    setTimeout(start, 1000);
    return;
  }
  const first = analyses[0];
  const rows = [];
  for (const result of results) {
    const row = document.createElement("tr");
    cell(row, result.name);
    cell(row, new Date(result.created).toLocaleTimeString());
    cell(row, result.provenance.nodes.map((n) => `${n.node} ${n.version}`).join(", "));
    cell(row, Object.values(result.provenance.hashes ?? {}).map((h) => h.slice(0, 15) + "…").join(" "));
    const files = cell(row, "");
    for (const file of result.files) {
      const link = document.createElement("a");
      link.href = `/api/v1/data/${found.id}/results/${result.id}/${file}`;
      link.textContent = file;
      files.append(link, " ");
    }
    const differs = cell(row, "");
    if (analyses.includes(result) && result !== first) {
      const changes = await read(`/api/v1/workspace/difference/${found.id}/${first.id}/${result.id}`);
      differs.textContent = changes.length
        ? changes.map((c) => `${c.path.split("/").slice(-3).join(".")}: ${JSON.stringify(c.before)} → ${JSON.stringify(c.after)}`).join("; ")
        : "the same";
    }
    rows.push(row);
  }
  $("results").replaceChildren(...rows);
  $("message").textContent = `: ${results.length} of ${found.name}, ${analyses.length} analyses`;
}

start().catch((error) => ($("message").textContent = `: ${error.message}`));
