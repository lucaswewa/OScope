// Shows the example's results.json: the slide's overview with the fields
// marked on it, and the fields by objective and focus.
"use strict";

function caption(field) {
  const sign = field.dz >= 0 ? "+" : "−";
  return `dz ${sign}${Math.abs(field.dz).toFixed(2)} µm · ${field.ms.toFixed(0)} ms · sharpness ${field.sharpness.toFixed(4)}`;
}

function image(file, alt) {
  const link = document.createElement("a");
  link.href = file;
  const img = document.createElement("img");
  img.src = file;
  img.alt = alt;
  img.loading = "lazy";
  link.append(img);
  return link;
}

async function start() {
  const response = await fetch("results.json");
  if (!response.ok) {
    throw new Error(`results.json answered ${response.status}`);
  }
  const results = await response.json();
  const [width, height] = results.field;
  const [cx, cy] = results.centre;
  document.getElementById("summary").textContent =
    `H&E tissue from seed ${results.seed}, on a slide whose corner is at (${results.slide[0]}, ${results.slide[1]}) µm on the stage. ` +
    `Each field is ${width} × ${height} pixels of the MIChrome 20's ${results.sensor_pixel} µm, centred at (${cx.toFixed(0)}, ${cy.toFixed(0)}) µm. ` +
    (results.repeatable ? "Rendered twice, a field is the same to the bit." : "A field rendered twice differed.");

  // The overview, with each field's outline at 5× and 10×.
  const overview = results.overview;
  const slide = document.getElementById("slide");
  slide.append(image(overview.file, "the whole slide"));
  const seen = new Set();
  for (const field of results.fields) {
    if (seen.has(field.objective) || !["5x", "10x"].includes(field.objective)) {
      continue;
    }
    seen.add(field.objective);
    const [sw, sh] = [overview.width * overview.pixel, overview.height * overview.pixel];
    const [fw, fh] = [width * field.pixel, height * field.pixel];
    const [x, y] = [cx - results.slide[0] - fw / 2, cy - results.slide[1] - fh / 2];
    const box = document.createElement("div");
    box.className = "box";
    box.title = field.objective;
    Object.assign(box.style, {
      left: `${(100 * x) / sw}%`,
      top: `${(100 * y) / sh}%`,
      width: `${(100 * fw) / sw}%`,
      height: `${(100 * fh) / sh}%`,
    });
    slide.append(box);
  }

  // The fields, a row for each objective.
  const table = document.getElementById("fields");
  const rows = new Map();
  for (const field of results.fields) {
    let row = rows.get(field.objective);
    if (!row) {
      row = table.insertRow();
      const head = document.createElement("th");
      head.innerHTML = `${field.objective}<br><span class="muted">NA ${field.na}<br>${field.pixel.toFixed(3)} µm a pixel<br>depth of field ${field.depth.toFixed(1)} µm</span>`;
      row.append(head);
      rows.set(field.objective, row);
    }
    const cell = row.insertCell();
    const figure = document.createElement("figure");
    figure.style.margin = "0";
    const text = document.createElement("figcaption");
    text.textContent = caption(field);
    figure.append(image(field.file, `${field.objective}, ${caption(field)}`), text);
    cell.append(figure);
  }
}

start().catch((error) => {
  const summary = document.getElementById("summary");
  summary.textContent = `The results couldn't be read: ${error.message}`;
  summary.className = "bad";
});
