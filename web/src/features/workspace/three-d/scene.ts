// The surface drawn by VTK.js (W7.7, ADR-0054). Only this module imports
// it, and only by a dynamic import, so it is a chunk of its own, loaded
// when a surface is first shown: lit, its texture the all-in-focus image,
// or coloured by height through a colour map; turned and zoomed by VTK.js's
// own camera controls.
import "@kitware/vtk.js/Rendering/Profiles/Geometry";

import vtkDataArray from "@kitware/vtk.js/Common/Core/DataArray";
import vtkImageData from "@kitware/vtk.js/Common/DataModel/ImageData";
import vtkPolyData from "@kitware/vtk.js/Common/DataModel/PolyData";
import vtkActor from "@kitware/vtk.js/Rendering/Core/Actor";
import vtkColorTransferFunction from "@kitware/vtk.js/Rendering/Core/ColorTransferFunction";
import vtkColorMaps from "@kitware/vtk.js/Rendering/Core/ColorTransferFunction/ColorMaps";
import vtkMapper from "@kitware/vtk.js/Rendering/Core/Mapper";
import vtkTexture from "@kitware/vtk.js/Rendering/Core/Texture";
import vtkGenericRenderWindow from "@kitware/vtk.js/Rendering/Misc/GenericRenderWindow";

import type { Mesh } from "./surface";

/** What colours the surface: the all-in-focus image, or its height through a map. */
export type Colouring = "image" | "height";

/** The colour maps offered: VTK.js's presets, by their names. */
export const MAPS = ["Viridis (matplotlib)", "Cool to Warm", "Grayscale"] as const;

export interface Scene {
  /** `mesh` drawn: at first, seen from above and tilted toward the viewer. */
  show(mesh: Mesh, heights: Float32Array): void;
  colour(by: Colouring, map: string): void;
  /** The camera as it first was. */
  reset(): void;
  /** Its size again, from its container's. */
  resize(): void;
  dispose(): void;
}

/** An RGB image, its first row at the top: the surface's texture. */
export interface Picture {
  width: number;
  height: number;
  rgb: Uint8Array;
}

/**
 * A scene in `container`, drawn on nothing: the container's own background
 * shows through, in either theme.
 */
export function sceneIn(container: HTMLElement, picture: Picture | undefined): Scene {
  const view = vtkGenericRenderWindow.newInstance({
    background: [0, 0, 0, 0],
    listenWindowResize: false,
  });
  view.setContainer(container);
  view.resize();
  const renderer = view.getRenderer();
  const window = view.getRenderWindow();
  const data = vtkPolyData.newInstance();
  const mapper = vtkMapper.newInstance();
  mapper.setInputData(data);
  const actor = vtkActor.newInstance();
  actor.setMapper(mapper);
  renderer.addActor(actor);
  let texture: ReturnType<typeof vtkTexture.newInstance> | undefined;
  if (picture) {
    const image = vtkImageData.newInstance();
    image.setDimensions(picture.width, picture.height, 1);
    const values = vtkDataArray.newInstance({ numberOfComponents: 3, values: picture.rgb });
    image.getPointData().setScalars(values);
    texture = vtkTexture.newInstance();
    texture.setInputData(image);
    texture.setInterpolate(true);
  }
  let range: [number, number] = [0, 1];
  let camera: { position: number[]; focal: number[]; up: number[] } | undefined;
  const scene: Scene = {
    show(mesh, heights) {
      range = mesh.range[1] > mesh.range[0] ? mesh.range : [mesh.range[0], mesh.range[0] + 1];
      data.getPoints().setData(mesh.points, 3);
      data.getPolys().setData(mesh.polys);
      const tcoords = vtkDataArray.newInstance({
        name: "tcoords",
        numberOfComponents: 2,
        values: mesh.tcoords,
      });
      data.getPointData().setTCoords(tcoords);
      const scalars = vtkDataArray.newInstance({
        name: "height",
        numberOfComponents: 1,
        values: heights,
      });
      data.getPointData().setScalars(scalars);
      data.modified();
      if (!camera) {
        renderer.resetCamera();
        renderer.getActiveCamera().elevation(-55);
        renderer.resetCamera();
        const now = renderer.getActiveCamera();
        camera = { position: now.getPosition(), focal: now.getFocalPoint(), up: now.getViewUp() };
      }
      window.render();
    },
    colour(by, map) {
      if (by === "image" && texture) {
        actor.addTexture(texture);
        mapper.setScalarVisibility(false);
      } else {
        actor.removeAllTextures();
        const table = vtkColorTransferFunction.newInstance();
        const preset = vtkColorMaps.getPresetByName(map);
        if (preset) table.applyColorMap(preset);
        table.setMappingRange(range[0], range[1]);
        table.updateRange();
        mapper.setLookupTable(table);
        mapper.setScalarVisibility(true);
        mapper.setUseLookupTableScalarRange(true);
      }
      window.render();
    },
    reset() {
      if (!camera) return;
      const now = renderer.getActiveCamera();
      now.setPosition(...(camera.position as [number, number, number]));
      now.setFocalPoint(...(camera.focal as [number, number, number]));
      now.setViewUp(...(camera.up as [number, number, number]));
      renderer.resetCameraClippingRange();
      window.render();
    },
    resize() {
      view.resize();
    },
    dispose() {
      view.delete();
    },
  };
  return scene;
}
