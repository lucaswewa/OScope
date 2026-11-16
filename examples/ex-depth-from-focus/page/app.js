// The newest depth from focus (P42): its heights, a float brick, drawn by
// VTK.js as a surface coloured by its all-in-focus image; the heights
// stretched if asked, and the cells not sure left out if asked.
"use strict";

const $ = (id) => document.getElementById(id);
// The cells at most: the server sends the finest level that holds them.
const MOST = 2000000;

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// The newest project's heights, with their confidence and all-in-focus
// image: the three results the job names alike.
async function newest() {
  const page = await read("/api/v1/projects/list");
  for (const found of page.projects.filter((s) => s.results > 0)) {
    const contents = await read(`/api/v1/projects/project/${found.id}`);
    const heights = contents.results.filter((r) => r.name.endsWith(" heights")).at(-1);
    if (!heights) continue;
    const stem = heights.name.slice(0, -" heights".length);
    const named = (what) => contents.results.filter((r) => r.name === `${stem} ${what}`).at(-1);
    const [confidence, image] = [named("confidence"), named("all in focus")];
    if (confidence && image) return { project: found.id, heights, confidence, image };
  }
  return null;
}

// A brick (ADR-0029): a header, little-endian, then its voxels, x fastest.
async function brick(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  const buffer = await answer.arrayBuffer();
  const view = new DataView(buffer);
  const u32 = (at) => view.getUint32(at, true);
  const start = view.getUint16(4, true);
  const kinds = { 1: Uint8Array, 2: Uint16Array, 4: Float32Array };
  return {
    level: u32(8),
    size: [u32(12), u32(16)],
    pixel: view.getFloat32(36, true) || 1,
    values: new kinds[view.getUint8(6)](buffer, start),
    bytes: buffer.byteLength,
  };
}

const drawn = {};

// The surface: a point at each cell, x to the right and y up the image, z
// its height above the lowest; two triangles to each square of four whose
// corners are sure enough, wound to face up; each point the image's colour.
function surface(heights, confidence, image, floor) {
  const [w, h] = heights.size;
  const z = heights.values;
  let lo = Infinity;
  for (const v of z) if (Number.isFinite(v) && v < lo) lo = v;
  const points = new Float32Array(3 * w * h);
  for (let i = 0; i < w * h; i++) {
    points[3 * i] = (i % w) * heights.pixel;
    points[3 * i + 1] = (h - 1 - Math.floor(i / w)) * heights.pixel;
    points[3 * i + 2] = Number.isFinite(z[i]) ? z[i] - lo : 0;
  }
  const polys = new Uint32Array(8 * (w - 1) * (h - 1));
  let n = 0;
  const sure = (i) => confidence.values[i] >= floor;
  for (let y = 0; y + 1 < h; y++) {
    for (let x = 0; x + 1 < w; x++) {
      const [tl, tr, bl, br] = [y * w + x, y * w + x + 1, (y + 1) * w + x, (y + 1) * w + x + 1];
      if (!(sure(tl) && sure(tr) && sure(bl) && sure(br))) continue;
      polys.set([3, bl, br, tr, 3, bl, tr, tl], n);
      n += 8;
    }
  }
  const data = vtk.Common.DataModel.vtkPolyData.newInstance();
  data.getPoints().setData(points, 3);
  data.getPolys().setData(polys.subarray(0, n));
  const colours = vtk.Common.Core.vtkDataArray.newInstance({
    name: "colours",
    numberOfComponents: 3,
    values: image.values,
  });
  data.getPointData().setScalars(colours);
  return { data, lo, triangles: n / 4 };
}

function draw(found, heights, confidence, image) {
  const screen = vtk.Rendering.Misc.vtkGenericRenderWindow.newInstance({ background: [0, 0, 0] });
  screen.setContainer($("view"));
  screen.resize();
  window.addEventListener("resize", () => screen.resize());
  const mapper = vtk.Rendering.Core.vtkMapper.newInstance();
  mapper.setColorModeToDirectScalars();
  mapper.setScalarModeToUsePointData();
  const actor = vtk.Rendering.Core.vtkActor.newInstance();
  actor.setMapper(mapper);
  const renderer = screen.getRenderer();
  renderer.addActor(actor);
  Object.assign(drawn, { mapper, actor, renderer, heights, confidence, image });
  drawn.render = () => screen.getRenderWindow().render();
  look();
  // Seen from above the chip and in front of it, the frame's top row at the
  // top.
  const [x0, x1, y0, y1, z0, z1] = drawn.data.getBounds();
  const centre = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
  const camera = renderer.getActiveCamera();
  camera.setFocalPoint(...centre);
  camera.setPosition(centre[0], centre[1] - 0.7, centre[2] + 0.7);
  camera.setViewUp(0, 0.7, 0.7);
  renderer.resetCamera();
  drawn.render();
}

// The look: the cells shown, and the depth's stretch.
function look() {
  const floor = $("sure").checked ? 0.5 : -1;
  const built = surface(drawn.heights, drawn.confidence, drawn.image, floor);
  drawn.data = built.data;
  drawn.mapper.setInputData(built.data);
  const depth = Number($("depth").value);
  $("depth-value").textContent = depth;
  drawn.actor.setScale(1, 1, depth);
  const [w, h] = drawn.heights.size;
  $("about").textContent =
    `Level ${drawn.heights.level}: ${w} × ${h} cells of ${drawn.heights.pixel.toFixed(3)} µm, ` +
    `${built.triangles} triangles, heights from ${built.lo.toFixed(2)} µm. Drag to turn it, scroll to zoom.`;
  if (drawn.render) drawn.render();
}

async function start() {
  const found = await newest();
  if (!found) {
    $("message").textContent = ": no heights yet; the example scans and finds them as it starts.";
    setTimeout(start, 3000);
    return;
  }
  const base = (record) => `/api/v1/volume/${found.project}/results/${record.id}/brick`;
  const heights = await brick(`${base(found.heights)}?format=f32&max=${MOST}`);
  const at = `?format=f32&level=${heights.level}`;
  const confidence = await brick(`${base(found.confidence)}${at}`);
  const image = await brick(`${base(found.image)}?level=${heights.level}`);
  $("message").textContent = `: ${found.heights.name}`;
  draw(found, heights, confidence, image);
  for (const id of ["depth", "sure"]) $(id).addEventListener("input", look);
}

start().catch((error) => {
  $("message").textContent = `: ${error.message}`;
  $("message").className = "bad";
});
