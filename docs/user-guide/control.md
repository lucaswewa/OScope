# Control

The live image and the instrument's controls: the panes on the left, the image on the right. The line between them moves, by the mouse or by its arrows.

## The live image

- **Start** streams the camera; **Stop** ends it. Under the image, its state, the frames a second, how late a frame is, the zoom, and the pixel under the pointer.
- **Zoom** with the wheel, **Zoom in** and **Zoom out**, or the keys. **Fit** shows the whole frame; **1:1** a sensor pixel on a screen pixel. Zoomed in, the camera sends the region shown, at its full detail.
- **Full screen** fills the screen; Escape leaves it. **Save PNG** saves the image as drawn.
- **What is drawn over it:** a crosshair, a grid, a scale bar, the saturated pixels in red, the histogram and the frame's values, a region drawn for the camera, and a line whose profile is charted.
- **The values,** with *Histogram*: the black and white points, gamma or sRGB's curve, a colour map, and the corrections: the dark frame taken off and the flat field divided out, where calibration has them.

## The camera

Its exposure and gain, its mode and pixel format, a region of the sensor, and its trigger. *Auto exposure* sets the exposure from the image, and *White balance* the colours, over something white.

## The lights

Each light on or off, and its intensity. The transmitted light is on by default.

## The objective

The objective fitted, from Mitutoyo's: changing it changes the scale. Where its calibration is missing or old, *Check the objective* measures it.

## The stage

- **Home** first: the stage waits to be homed after each start. *Centre* goes to the middle of its travel.
- **Move to:** type x, y and z, in µm or mm, and press *Move*.
- **Steps:** choose a step for x and y, and one for z, then the arrows, on the pad or on the image. *While held* moves as long as the key or button is held.
- **Pointing:** with the geometry calibrated, a double click on the image sends the stage there, and a drag moves it.
- **Halt,** or Escape, stops it at once.
- **The speed** for moves, from a slow approach to the stage's own.

## Autofocus

*Focus* sweeps z about where the stage is, and leaves it where the image is sharpest: its curve, its peak and where it was found are shown. Its measure, range and steps are chosen in its pane.

## Capture

Choose a project, or make one under *New project*, name what is captured, and *Capture*: a frame, or a burst of frames. Its thumbnail shows, with a link to it in the Gallery.

## A live recipe

A recipe run on each frame in place of the look: one that comes with OScope, one of the library's, or one opened from a file. Its cost is said against its budget.

## The mosaic

*Mosaic* opens it beside the image. *Start the mosaic*, then move the stage: a frame is drawn each time the stage stops somewhere new, and the field of view's rectangle follows. A double click on the mosaic sends the stage there. *Clear* starts it anew.
