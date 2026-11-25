# OScope: the user's guide

OScope runs the microscope from a web browser: the camera, the stage, the lights and the objectives, the scans, the analyses and the reports. This guide takes it a tab at a time.

## Opening it

- **On the microscope's computer:** open `http://127.0.0.1:8210/` in a browser: Chrome, Edge, Firefox or Safari.
- **From another computer,** where the administrator has opened it to the network ([deploying on Ubuntu](../deploying-on-ubuntu.md)): its address, as the administrator gives it. It then asks you to log in.
- **The rail,** down the left side, holds the tabs. At its foot: whether the server is online, the notices, and the instrument's state. Press the state for what each part of the instrument says, and *The status in full*.

## The tabs

| Tab | What it is for |
|---|---|
| [Control](control.md) | The live image, the camera, the lights, the objective and the stage; focusing, capturing, a live recipe and a mosaic built by moving |
| [Scan](scan.md) | Planning a scan and running it; the histo scan, from an overview to a stitched slide |
| [Workspace](workspace.md) | Recipes drawn as graphs; analyses run on what was taken; shapes drawn and measured; surfaces and volumes in 3D; reports |
| [Gallery](gallery.md) | The projects and what they hold; an image viewed at every zoom; files imported and exported |
| [Jobs](jobs.md) | What the instrument is doing, and what waits |
| [Settings](settings.md) | The display, the system, the devices, calibration, the simulator, extensions, accounts |
| [Logging](logging.md) | The server's log |
| [About](about.md) | The versions, the computer and the licences |
| [Power](power.md) | Restarting the service, rebooting and shutting down |

## Working with the keyboard

Everything works without a mouse:

- **Tab** moves from one control to the next; **Shift and Tab** go back. A list of tabs is one stop: its arrows move along it.
- **Away from a field,** the digits **1 to 9** open the tabs in the rail's order, and **?** lists the shortcuts.
- **On the live image:** the arrows step the stage, Page Up and Page Down move it in z, **+** and **−** zoom, **0** fits the image, **1** shows the sensor's pixels one to one, and **F** fills the screen.
- **Escape** closes a dialog or a menu, and halts the stage on Control.

## Taking control

Where the server has accounts, one person at a time controls the instrument: *Take control* in the rail. Others watch, and may ask for control: its holder is asked to hand it over. Control lapses after a while idle, ten minutes by default.

## The look

*Settings*, *Display*: the theme, light, dark or the system's; an accent colour; lengths in µm or mm; a scale bar on images; and a colour map for one-channel images. They follow your account to every browser.
