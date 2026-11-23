// A volume drawn by VTK.js (W7.8, ADR-0054): ray cast, one channel, through
// its transfer function's opacity and a colour map; clipped by a box; the
// three slices' planes outlined in it; and its camera followed by another's,
// so two volumes are seen alike. Only this module and `scene.ts` import
// VTK.js, each by a dynamic import.
import "@kitware/vtk.js/Rendering/Profiles/Geometry";
import "@kitware/vtk.js/Rendering/Profiles/Volume";

import vtkDataArray from "@kitware/vtk.js/Common/Core/DataArray";
import vtkCellArray from "@kitware/vtk.js/Common/Core/CellArray";
import vtkImageData from "@kitware/vtk.js/Common/DataModel/ImageData";
import vtkPiecewiseFunction from "@kitware/vtk.js/Common/DataModel/PiecewiseFunction";
import vtkPlane from "@kitware/vtk.js/Common/DataModel/Plane";
import vtkPolyData from "@kitware/vtk.js/Common/DataModel/PolyData";
import vtkActor from "@kitware/vtk.js/Rendering/Core/Actor";
import vtkColorTransferFunction from "@kitware/vtk.js/Rendering/Core/ColorTransferFunction";
import vtkColorMaps from "@kitware/vtk.js/Rendering/Core/ColorTransferFunction/ColorMaps";
import vtkMapper from "@kitware/vtk.js/Rendering/Core/Mapper";
import vtkVolume from "@kitware/vtk.js/Rendering/Core/Volume";
import vtkVolumeMapper from "@kitware/vtk.js/Rendering/Core/VolumeMapper";
import vtkGenericRenderWindow from "@kitware/vtk.js/Rendering/Misc/GenericRenderWindow";

import { nodesOf, type Point } from "./transfer";

type Triple = [number, number, number];

/** One channel of a volume: its size, its voxels' size in µm, and its samples, x fastest. */
export interface Channel {
  size: Triple;
  voxelUm: Triple;
  samples: Float32Array;
}

/** Where a camera is: its position, what it looks at, and which way is up. */
export interface Camera {
  position: Triple;
  focal: Triple;
  up: Triple;
}

/** A part of each axis, from 0 to 1. */
export type Box = Record<"x" | "y" | "z", [number, number]>;

export interface VolumeScene {
  /** Its look: the transfer function's points over `range`, coloured by `map`. */
  look(points: readonly Point[], range: [number, number], map: string): void;
  clip(box: Box): void;
  /** The three slices' planes, each a part of its axis. */
  slices(at: Record<"x" | "y" | "z", number>): void;
  /** Its camera; told it as it changes; and set from another's. */
  camera(): Camera;
  onCamera(listener: (camera: Camera) => void): void;
  follow(camera: Camera): void;
  reset(): void;
  resize(): void;
  dispose(): void;
}

/** `channel` drawn in `container`, on nothing: the container's background shows through. */
export function volumeIn(container: HTMLElement, channel: Channel): VolumeScene {
  const view = vtkGenericRenderWindow.newInstance({
    background: [0, 0, 0, 0],
    listenWindowResize: false,
  });
  view.setContainer(container);
  view.resize();
  const renderer = view.getRenderer();
  const window = view.getRenderWindow();
  const image = vtkImageData.newInstance();
  image.setDimensions(...channel.size);
  image.setSpacing(channel.voxelUm);
  image
    .getPointData()
    .setScalars(vtkDataArray.newInstance({ numberOfComponents: 1, values: channel.samples }));
  const mapper = vtkVolumeMapper.newInstance();
  mapper.setInputData(image);
  mapper.setSampleDistance(Math.min(...channel.voxelUm) / 2);
  const volume = vtkVolume.newInstance();
  volume.setMapper(mapper);
  const property = volume.getProperty();
  property.setInterpolationTypeToLinear();
  renderer.addVolume(volume);

  // The slices' planes: three rectangles' outlines.
  const extent = channel.size.map((n, i) => (n - 1) * channel.voxelUm[i]!) as Triple;
  const outlines = vtkPolyData.newInstance();
  const outlined = vtkMapper.newInstance();
  outlined.setInputData(outlines);
  const frames = vtkActor.newInstance();
  frames.setMapper(outlined);
  frames.getProperty().setColor(0.85, 0.2, 0.55);
  renderer.addActor(frames);

  renderer.resetCamera();
  renderer.getActiveCamera().elevation(-30);
  renderer.getActiveCamera().azimuth(30);
  renderer.resetCamera();
  const camera = renderer.getActiveCamera();
  const now = (): Camera => ({
    position: camera.getPosition() as Triple,
    focal: camera.getFocalPoint() as Triple,
    up: camera.getViewUp() as Triple,
  });
  let following = false;
  const listeners: ((camera: Camera) => void)[] = [];
  camera.onModified(() => {
    if (following) return;
    for (const listener of listeners) listener(now());
  });
  const start = now();
  const set = (to: Camera) => {
    camera.setPosition(...to.position);
    camera.setFocalPoint(...to.focal);
    camera.setViewUp(...to.up);
    renderer.resetCameraClippingRange();
    window.render();
  };

  return {
    look(points, range, map) {
      const [least, most] = range;
      const opacity = vtkPiecewiseFunction.newInstance();
      for (const [value, alpha] of nodesOf(points, range)) opacity.addPoint(value, alpha);
      const colours = vtkColorTransferFunction.newInstance();
      const preset = vtkColorMaps.getPresetByName(map);
      if (preset) colours.applyColorMap(preset);
      colours.setMappingRange(least, most);
      colours.updateRange();
      property.setRGBTransferFunction(0, colours);
      property.setScalarOpacity(0, opacity);
      window.render();
    },
    clip(box) {
      mapper.removeAllClippingPlanes();
      const axes = ["x", "y", "z"] as const;
      axes.forEach((axis, i) => {
        const [from, to] = box[axis];
        const normal = [0, 0, 0] as Triple;
        const origin = [0, 0, 0] as Triple;
        normal[i] = 1;
        origin[i] = from * extent[i]!;
        mapper.addClippingPlane(vtkPlane.newInstance({ normal: [...normal], origin: [...origin] }));
        normal[i] = -1;
        origin[i] = to * extent[i]!;
        mapper.addClippingPlane(vtkPlane.newInstance({ normal: [...normal], origin: [...origin] }));
      });
      window.render();
    },
    slices(at) {
      const [ex, ey, ez] = extent;
      const [x, y, z] = [at.x * ex, at.y * ey, at.z * ez];
      const corners = [
        // The z slice, then the y, then the x.
        [0, 0, z],
        [ex, 0, z],
        [ex, ey, z],
        [0, ey, z],
        [0, y, 0],
        [ex, y, 0],
        [ex, y, ez],
        [0, y, ez],
        [x, 0, 0],
        [x, ey, 0],
        [x, ey, ez],
        [x, 0, ez],
      ];
      outlines.getPoints().setData(Float32Array.from(corners.flat()), 3);
      const lines = [0, 4, 8].flatMap((k) => [5, k, k + 1, k + 2, k + 3, k]);
      outlines.setLines(vtkCellArray.newInstance({ values: Uint32Array.from(lines) }));
      outlines.modified();
      window.render();
    },
    camera: now,
    onCamera(listener) {
      listeners.push(listener);
    },
    follow(to) {
      following = true;
      set(to);
      following = false;
    },
    reset() {
      set(start);
    },
    resize() {
      view.resize();
    },
    dispose() {
      view.delete();
    },
  };
}
