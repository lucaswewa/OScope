// The newest deconvolution (P43) and the stack it came from, drawn by
// VTK.js: as a maximum intensity projection, composited through a transfer
// function, or as an isosurface (marching cubes), with a toggle between
// before and after. Each volume's window runs from its median to its
// 99.9th percentile; the level slider sets the white, or the surface's
// value, within it.
"use strict";

const $ = (id) => document.getElementById(id);

async function read(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// The newest result deconvolved, of the newest project with one, and the
// acquisition it names.
async function newest() {
  const page = await read("/api/v1/projects/list");
  for (const found of page.projects.filter((s) => s.results > 0)) {
    const contents = await read(`/api/v1/projects/project/${found.id}`);
    const result = contents.results.filter((r) => r.name.endsWith(" deconvolved")).at(-1);
    const input = result?.provenance.inputs.find((i) => i.kind === "acquisition");
    if (input) return { project: found.id, result, acquisition: input.id };
  }
  return null;
}

// A float brick (ADR-0029): its size, voxel and values, and its window.
async function brick(path) {
  const answer = await fetch(path);
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  const buffer = await answer.arrayBuffer();
  const view = new DataView(buffer);
  const u32 = (at) => view.getUint32(at, true);
  const values = new Float32Array(buffer, view.getUint16(4, true));
  const sample = [];
  const step = Math.max(1, Math.floor(values.length / 100000));
  for (let i = 0; i < values.length; i += step) sample.push(values[i]);
  sample.sort((a, b) => a - b);
  const at = (q) => sample[Math.floor(q * (sample.length - 1))];
  return {
    size: [u32(12), u32(16), u32(20)],
    voxel: [view.getFloat32(36, true), view.getFloat32(40, true), view.getFloat32(44, true)].map((v) => v || 1),
    values,
    low: at(0.5),
    high: Math.max(at(0.999), at(0.5) + 1e-6),
  };
}

const drawn = {};

function imageOf(volume) {
  const image = vtk.Common.DataModel.vtkImageData.newInstance();
  image.setDimensions(...volume.size);
  image.setSpacing(...volume.voxel);
  const scalars = vtk.Common.Core.vtkDataArray.newInstance({ name: "intensity", numberOfComponents: 1, values: volume.values });
  image.getPointData().setScalars(scalars);
  return image;
}

function setUp(volumes) {
  const { vtkVolume, vtkVolumeMapper, vtkColorTransferFunction, vtkMapper, vtkActor } = vtk.Rendering.Core;
  const screen = vtk.Rendering.Misc.vtkGenericRenderWindow.newInstance({ background: [0, 0, 0] });
  screen.setContainer($("view"));
  screen.resize();
  window.addEventListener("resize", () => screen.resize());
  const renderer = screen.getRenderer();
  const images = { before: imageOf(volumes.before), after: imageOf(volumes.after) };
  const mapper = vtkVolumeMapper.newInstance();
  const volume = vtkVolume.newInstance();
  volume.setMapper(mapper);
  const colour = vtkColorTransferFunction.newInstance();
  const opacity = vtk.Common.DataModel.vtkPiecewiseFunction.newInstance();
  volume.getProperty().setRGBTransferFunction(0, colour);
  volume.getProperty().setScalarOpacity(0, opacity);
  volume.getProperty().setInterpolationTypeToLinear();
  const cubes = vtk.Filters.General.vtkImageMarchingCubes.newInstance({ computeNormals: true, mergePoints: true });
  const surfaceMapper = vtkMapper.newInstance();
  surfaceMapper.setInputConnection(cubes.getOutputPort());
  const surface = vtkActor.newInstance();
  surface.setMapper(surfaceMapper);
  surface.getProperty().setColor(0.4, 1, 0.5);
  renderer.addVolume(volume);
  renderer.addActor(surface);
  Object.assign(drawn, { volumes, images, mapper, volume, colour, opacity, cubes, surface, renderer });
  drawn.render = () => screen.getRenderWindow().render();
  look();
  // Seen from above the slide and in front of it, the frame's top row at
  // the top.
  const [x0, x1, y0, y1, z0, z1] = images.after.getBounds();
  const centre = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
  const camera = renderer.getActiveCamera();
  camera.setFocalPoint(...centre);
  camera.setPosition(centre[0], centre[1] + 0.6, centre[2] + 0.8);
  camera.setViewUp(0, -0.8, 0.6);
  renderer.resetCamera();
  drawn.render();
}

// The look: which volume, how it is drawn, its level and its depth.
function look() {
  const { volumes, images, mapper, volume, colour, opacity, cubes, surface } = drawn;
  const which = $("which").value;
  const shown = volumes[which];
  const level = shown.low + Number($("level").value) * (shown.high - shown.low);
  const mode = $("mode").value;
  mapper.setInputData(images[which]);
  colour.removeAllPoints();
  colour.addRGBPoint(shown.low, 0, 0, 0);
  colour.addRGBPoint(level, 0.4, 1, 0.5);
  opacity.removeAllPoints();
  if (mode === "mip") {
    mapper.setBlendModeToMaximumIntensity();
    opacity.addPoint(shown.low, 1);
    opacity.addPoint(shown.high, 1);
  } else {
    // Clear below the window, half opaque a µm at the level.
    mapper.setBlendModeToComposite();
    opacity.addPoint(shown.low, 0);
    opacity.addPoint(level, 0.5);
  }
  cubes.setInputData(images[which]);
  cubes.setContourValue(level);
  volume.setVisibility(mode !== "surface");
  surface.setVisibility(mode === "surface");
  const depth = Number($("depth").value);
  volume.setScale(1, 1, depth);
  surface.setScale(1, 1, depth);
  const [w, h, d] = shown.size;
  $("about").textContent =
    `${which === "after" ? "Deconvolved" : "The stack"}: ${w} × ${h} × ${d} voxels of ` +
    `${shown.voxel.map((v) => +v.toFixed(3)).join(" × ")} µm. Drag to turn it, scroll to zoom.`;
  drawn.render();
}

async function start() {
  const found = await newest();
  if (!found) {
    $("message").textContent = ": none yet; the example scans and deconvolves as it starts.";
    setTimeout(start, 3000);
    return;
  }
  const base = `/api/v1/volume/${found.project}`;
  const after = await brick(`${base}/results/${found.result.id}/brick?format=f32&level=0`);
  const before = await brick(`${base}/acquisitions/${found.acquisition}/brick?format=f32&level=1`);
  $("message").textContent = `: ${found.result.name}`;
  setUp({ before, after });
  for (const id of ["which", "mode", "level", "depth"]) $(id).addEventListener("input", look);
}

start().catch((error) => {
  $("message").textContent = `: ${error.message}`;
  $("message").className = "bad";
});
