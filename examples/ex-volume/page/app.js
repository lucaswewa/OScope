// The newest volume (P41), a z-stack among the projects' acquisitions,
// drawn by VTK.js from a brick of its voxels: as a maximum intensity
// projection or composited, through a window of its intensities, its depth
// stretched if asked.
"use strict";

const $ = (id) => document.getElementById(id);
// The brick's voxels at most: the server sends the finest level that fits.
const MOST = 8000000;

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// The newest acquisition that is a volume, of the newest project with one.
async function newest() {
  const page = await read("/api/v1/projects/list");
  for (const found of page.projects.filter((s) => s.acquisitions > 0)) {
    const contents = await read(`/api/v1/projects/project/${found.id}`);
    const volume = contents.acquisitions.filter((a) => a.image.volume).at(-1);
    if (volume) return { project: found.id, acquisition: volume };
  }
  return null;
}

// A brick (ADR-0029): a header, little-endian, then its voxels, x fastest.
async function brickOf(base) {
  const answer = await fetch(`${base}/brick?max=${MOST}`);
  if (!answer.ok) throw new Error(`the brick: ${answer.status}`);
  const buffer = await answer.arrayBuffer();
  const view = new DataView(buffer);
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, 4));
  if (magic !== "OSV1") throw new Error("the brick has no header");
  const u32 = (at) => view.getUint32(at, true);
  const f32 = (at) => view.getFloat32(at, true);
  const start = view.getUint16(4, true);
  const wide = view.getUint8(6) === 2;
  return {
    level: u32(8),
    size: [u32(12), u32(16), u32(20)],
    // A voxel of unknown size is drawn as a cube.
    voxel: [f32(36), f32(40), f32(44)].map((v) => v || 1),
    values: wide ? new Uint16Array(buffer, start) : new Uint8Array(buffer, start),
    top: wide ? 65535 : 255,
    bytes: buffer.byteLength,
  };
}

// The window to start from, on the sliders' scale: black at the median of
// a sample of the voxels, the glass between the cells, and white at its
// 99.9th percentile.
function windowOf(brick) {
  const { values, top } = brick;
  const step = Math.max(1, Math.floor(values.length / 100000));
  const sample = [];
  for (let i = 0; i < values.length; i += step) sample.push(values[i]);
  sample.sort((a, b) => a - b);
  const at = (q) => Math.round((sample[Math.floor(q * (sample.length - 1))] * 255) / top);
  return [at(0.5), Math.max(at(0.999), at(0.5) + 1)];
}

const drawn = {};

function draw(brick) {
  const { vtkVolume, vtkVolumeMapper, vtkColorTransferFunction } = vtk.Rendering.Core;
  const { vtkImageData, vtkPiecewiseFunction } = vtk.Common.DataModel;
  const screen = vtk.Rendering.Misc.vtkGenericRenderWindow.newInstance({ background: [0, 0, 0] });
  screen.setContainer($("view"));
  screen.resize();
  window.addEventListener("resize", () => screen.resize());
  const image = vtkImageData.newInstance();
  image.setDimensions(...brick.size);
  image.setSpacing(...brick.voxel);
  const scalars = vtk.Common.Core.vtkDataArray.newInstance({
    name: "intensity",
    numberOfComponents: 1,
    values: brick.values,
  });
  image.getPointData().setScalars(scalars);
  const mapper = vtkVolumeMapper.newInstance();
  mapper.setInputData(image);
  mapper.setSampleDistance(Math.min(...brick.voxel) / 2);
  const volume = vtkVolume.newInstance();
  volume.setMapper(mapper);
  const colour = vtkColorTransferFunction.newInstance();
  const opacity = vtkPiecewiseFunction.newInstance();
  volume.getProperty().setRGBTransferFunction(0, colour);
  volume.getProperty().setScalarOpacity(0, opacity);
  volume.getProperty().setInterpolationTypeToLinear();
  const renderer = screen.getRenderer();
  renderer.addVolume(volume);
  // Seen from above the slide and in front of it, the frame's top row at
  // the top: the image's y runs down, and z up from the glass.
  const [x0, x1, y0, y1, z0, z1] = image.getBounds();
  const centre = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
  const camera = renderer.getActiveCamera();
  camera.setFocalPoint(...centre);
  camera.setPosition(centre[0], centre[1] + 0.6, centre[2] + 0.8);
  camera.setViewUp(0, -0.8, 0.6);
  renderer.resetCamera();
  Object.assign(drawn, { colour, opacity, mapper, volume, top: brick.top });
  drawn.render = () => screen.getRenderWindow().render();
  [$("low").value, $("high").value] = windowOf(brick);
  look();
}

// The look: how it is drawn, the window of intensities from black to the
// dye's green, and the depth's stretch.
function look() {
  const { colour, opacity, mapper, volume, top } = drawn;
  const scale = top / 255;
  const low = Number($("low").value) * scale;
  const high = Math.max(Number($("high").value), Number($("low").value) + 1) * scale;
  colour.removeAllPoints();
  colour.addRGBPoint(low, 0, 0, 0);
  colour.addRGBPoint(high, 0.35, 1, 0.45);
  opacity.removeAllPoints();
  if ($("mode").value === "mip") {
    mapper.setBlendModeToMaximumIntensity();
    opacity.addPoint(0, 1);
    opacity.addPoint(top, 1);
  } else {
    // Clear below the window, half opaque a µm at its top.
    mapper.setBlendModeToComposite();
    opacity.addPoint(low, 0);
    opacity.addPoint(high, 0.5);
  }
  volume.setScale(1, 1, Number($("depth").value));
  drawn.render();
}

async function start() {
  const found = await newest();
  if (!found) {
    $("message").textContent = ": no z-stack yet; the example takes one as it starts.";
    setTimeout(start, 3000);
    return;
  }
  const { project, acquisition } = found;
  const base = `/api/v1/volume/${project}/acquisitions/${acquisition.id}`;
  const meta = await read(`${base}/meta`);
  const voxel = meta.voxel_um ? `of ${meta.voxel_um.map((v) => +v.toFixed(3)).join(" × ")} µm` : "of unknown size";
  $("message").textContent = `: ${acquisition.name}, ${meta.size.join(" × ")} voxels ${voxel}`;
  const started = performance.now();
  const brick = await brickOf(base);
  const ms = performance.now() - started;
  draw(brick);
  $("brick").textContent =
    `Drawn from level ${brick.level}, ${brick.size.join(" × ")} voxels, ` +
    `${(brick.bytes / 1e6).toFixed(1)} MB fetched in ${ms.toFixed(0)} ms. Drag to turn it, scroll to zoom.`;
  for (const id of ["mode", "low", "high", "depth"]) $(id).addEventListener("input", look);
}

start().catch((error) => {
  $("message").textContent = `: ${error.message}`;
  $("message").className = "bad";
});
