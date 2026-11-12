// Shows the example's results.json: each method's times against §5.20's
// budgets, its PSNR, whether its CUDA twin matched, and its images.
"use strict";

// A time, and whether it is within its budget.
function time(cell, ms, budget) {
  if (ms === null) {
    cell.textContent = "—";
    return;
  }
  const within = ms <= budget;
  cell.textContent = `${ms.toFixed(1)} ms ${within ? "✓" : "✗"}`;
  cell.className = within ? "good" : "bad";
  cell.title = `the budget is ${budget} ms`;
}

function figure(file, caption, download) {
  const image = document.createElement("img");
  image.src = file;
  image.alt = caption;
  image.loading = "lazy";
  const text = document.createElement("figcaption");
  text.textContent = caption;
  if (download) {
    const link = document.createElement("a");
    link.href = download;
    link.textContent = "16-bit TIFF";
    text.append(" · ", link);
  }
  const box = document.createElement("figure");
  box.append(image, text);
  return box;
}

async function start() {
  const response = await fetch("results.json");
  if (!response.ok) {
    throw new Error(`results.json answered ${response.status}`);
  }
  const results = await response.json();
  const [width, height] = results.chart;
  const gpu = results.gpu ? `the GPU is ${results.gpu}` : "there is no GPU in use";
  document.getElementById("summary").textContent =
    `A ${width} × ${height} test chart, mosaicked as RGGB16. The CPU used ${results.threads} threads, and ${gpu}. ` +
    `Each time is the median of ${results.runs} runs, from the raw frame to RGB.`;

  const table = document.getElementById("results");
  for (const method of results.methods) {
    const row = table.insertRow();
    row.insertCell().textContent = method.name;
    time(row.insertCell(), method.cpu_ms, results.budgets.cpu_ms);
    time(row.insertCell(), method.gpu_ms, results.budgets.gpu_ms);
    row.insertCell().textContent = `${method.psnr.toFixed(1)} dB`;
    const same = row.insertCell();
    same.textContent = method.matches_cpu === null ? "—" : method.matches_cpu ? "the same, to the bit" : "different";
    if (method.matches_cpu !== null) {
      same.className = method.matches_cpu ? "good" : "bad";
    }
    row.insertCell().textContent = `${method.width} × ${method.height}`;

    document.getElementById("rings").append(
      figure(`${method.slug}-rings.png`, method.name, `${method.slug}-rings.tif`),
    );
    document.getElementById("whole").append(figure(`${method.slug}.png`, method.name));
  }
}

start().catch((error) => {
  const summary = document.getElementById("summary");
  summary.textContent = `The results couldn't be read: ${error.message}`;
  summary.className = "bad";
});
