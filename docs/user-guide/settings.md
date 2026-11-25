# Settings

A view each, along the top.

## Display

Your own, following your account to every browser: the theme, an accent colour, lengths in µm or mm, the scale bar on images, and the colour map for one-channel images. *Back to the defaults* resets them.

## System

What the server starts with next, from its configuration file: the data and scratch folders, the address it listens on and its port. *Save*, then restart from Power. Listening on the network needs accounts.

## Devices

What the devices keep between starts: the camera's settings, the lights, the objective fitted, and the stage's speed. *Forget them* goes back to the defaults at the next start.

## Calibration

- **The wizard:** *Start the wizard*, and it takes you through the objective, the geometry, a dark frame, a flat field, colour and the stage, a step at a time, each measured beside the live image. A step can be skipped, or tried again where it failed; the wizard is kept by the server, so a reload or another browser finds it where it was.
- **The routines,** one at a time: what one measures is in use at once, for the settings it was measured at.
- **Every set,** by objective: its versions, which is in use, and *Use this one*. The geometry's sets export and import.
- **The results:** a flat field's map, and the geometry's residuals, charted.

## Simulation

With the simulated instrument:

- **The samples:** each with its picture; *Load it* puts it on the stage. *Choose a file…* imports your own image as a sample, at the pixel size you give.
- **Its clock:** faster runs moves sooner.
- **At the next start:** the simulator's camera and stage, such as the backlash, saved for the next start.
- **For trying things:** where the sample is in focus, and faults to inject and clear.

## Extensions

*Install a package…*, then restart from Power to load it. Each extension says what it brings: nodes, patterns, panels. A panel opens at its own address. *Disable*, *Enable* and *Uninstall* take effect at the next start.

## Accounts

- **The accounts:** add one, a name and a password typed twice; remove one, asked first. The first account turns security on at the next start, and the last off.
- **Your password,** changed with the old one.
- **The sessions open,** and how long a session and control may stay idle.

## The whole document

Every setting in one file: *Export* it, and *Import* it here or on another instrument.
