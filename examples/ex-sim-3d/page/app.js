// Shows the example's results.json: the surface's and the cells' z-series,
// each with a slider, beside their ground truth.
"use strict";

function microns(v) {
  return `${v.toFixed(2)} µm`;
}

// Wires the slider `id` to `planes`, calling `show` with the chosen plane.
function stepper(id, planes, start, show) {
  const slider = document.getElementById(id);
  slider.max = planes.length - 1;
  slider.value = start;
  const update = () => {
    const plane = planes[Number(slider.value)];
    document.getElementById(`${id}-text`).textContent = microns(plane.z);
    show(plane);
  };
  slider.addEventListener("input", update);
  update();
}

async function start() {
  const response = await fetch("results.json");
  if (!response.ok) {
    throw new Error(`results.json answered ${response.status}`);
  }
  const results = await response.json();
  const surface = results.surface;
  const cells = results.cells;
  document.getElementById("summary").textContent =
    `Seed ${results.seed}. Each z is above the slide's surface. ` +
    (results.repeatable ? "Rendered twice, a plane is the same to the bit." : "A plane rendered twice differed.");

  const [sw, sh] = surface.field;
  document.getElementById("surface-note").textContent =
    `A die of the silicon chip at 20× (NA ${surface.na}), ${sw} × ${sh} pixels of ${surface.pixel.toFixed(3)} µm: ` +
    `the stepped pyramid top left, the dome top right, the ridge bottom left, aluminium with etched pits bottom right. ` +
    `Its heights run from ${microns(surface.range[0])} to ${microns(surface.range[1])}. ` +
    `Depth from focus is within ${microns(surface.median_error)} of the truth for half the pixels, ` +
    `and within 0.25 µm for ${(100 * surface.within).toFixed(0)}%; it misses at steps and edges.`;
  document.getElementById("surface-truth").src = surface.truth;
  document.getElementById("surface-found").src = surface.found;
  stepper("surface-z", surface.planes, Math.floor(surface.planes.length / 2), (plane) => {
    document.getElementById("surface-plane").src = plane.file;
  });

  const [cw, ch] = cells.field;
  document.getElementById("cells-note").textContent =
    `${cells.cells_in_view} cells through a ${cells.objective}, ${cw} × ${ch} pixels of ${cells.pixel.toFixed(3)} µm, ` +
    `each image the violet, blue and green lines' added. The confocal's optical section is ${microns(cells.section)} ` +
    `(${microns(cells.dry_section)} through the instrument's dry 50×). Both are shown on the same scale.`;
  stepper("cells-z", cells.planes, Math.floor(cells.planes.length / 3), (plane) => {
    document.getElementById("cells-wide").src = plane.widefield;
    document.getElementById("cells-confocal").src = plane.confocal;
    document.getElementById("cells-truth").src = plane.truth;
    document.getElementById("cells-wide-caption").textContent =
      `Widefield: every depth's light, blurred. Mean ${plane.widefield_light.toFixed(4)}.`;
    document.getElementById("cells-confocal-caption").textContent =
      `Confocal: this depth's light. Mean ${plane.confocal_light.toFixed(4)}.`;
  });
}

start().catch((error) => {
  const summary = document.getElementById("summary");
  summary.textContent = `Couldn't show the results: ${error.message}`;
  summary.className = "bad";
});
