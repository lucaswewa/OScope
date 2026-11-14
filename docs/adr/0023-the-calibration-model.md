# ADR-0023: The calibration model

- Status: Accepted
- Date: 2026-11-14
- Phase: P32

## Context

- **D18:** objectives are swapped by hand, and each has its own calibration. Plan §5.13 keeps calibration per objective, and per binning where it matters, versioned, with a date, the account and the instrument's state; sets are listed, activated, exported and imported.
- **P32** measures the geometry (the pixel size and the camera-to-stage affine) and the stage (backlash, soft limits, homing). **P33** adds dark frames, flat fields, white balance and the colour matrix. Scans, stitching and the live view's click-to-move use them later.
- **The instrument moves under the camera.** A stepper stage reports where it was told to go. What the camera sees is where it is, so only images measure backlash.

## Decision

**1. A calibration set** is one calibration's values, as made:

- **an ID** (a UUID), **a kind**, **a key** within the kind, and **a version** for that key, from 1;
- **when** it was made, and by **which account**;
- **the instrument's state:** the camera's model and sensor pixel, the binning, the stage's model, the objective (name, magnification, NA), and the tube lens;
- **its values;**
- **where it came from,** when it was imported.

| Kind | Key | Values |
|---|---|---|
| `geometry` | the objective and the binning | the nominal pixel size; the matrix M; the pixel size, rotation, mirroring, aspect and skew it decomposes into; the fit's residual; the moves |
| `stage` | the instrument | the backlash of x and y; the soft limits; where homing left the stage |

P33 adds its kinds beside these.

**2. The camera-to-stage matrix M,** in µm per pixel:

- **Its use:** a displacement d in the image, in pixels, x right and y down, is brought to the image's centre by the stage move M · d.
- **Equivalently,** a stage move s carries the content by −M⁻¹ · s.
- **Its parts:** M = R(θ) · [[s_x, k], [0, s_y]] · diag(m, 1), with m = −1 for a mirrored image.
- **What it gives:** the pixel size √|det M|, the rotation θ, mirrored, the aspect s_x / s_y and the skew k / √(s_x s_y).

**3. Versions are kept.** A set is never changed or removed. A new one, measured or imported, is the next version of its key.

**4. One active set per key.** Calibration's users take the active set: the fitted objective's geometry at the camera's binning, and the stage's. A measured set is made active unless asked not to be; an imported one is not.

**5. The store** is a folder, `calibration/` under the data root:

- each set is a JSON file named by its ID;
- `active.json` maps each key to its active set;
- each file is written whole and renamed into place;
- the folder is read when the server starts.

**6. Export and import** move a set's JSON between instruments. An import is checked as a set must be, then saved as the next version of its key, recording the ID it came from.

**7. Measuring** is a job on the instrument, holding it while the stage moves (ADR-0016):

- **the geometry,** from a 3 × 3 grid of moves, each position reached from the same side;
- **the stage's backlash,** from a position reached from either side;
- **an objective check,** from two moves.

The behaviour, with its sources, is P32's notes' spec.

## Consequences

- **A calibration made for the wrong objective is refused,** as its pixel size is far from the nominal. The objective check catches the swap that wasn't recorded.
- **Calibrating doesn't change history:** acquisitions keep the values they were made with, and an earlier set can be made active again.
- **The store is small:** a set is a few kilobytes, and a folder read at start is enough. Thousands of versions would want an index.
- **Sets from another instrument** import, and carry its state, so a reader can see where they came from. They aren't refused for a different camera or stage, as a twin instrument's calibration is a fair start.
- **Backlash is measured in x and y.** z's needs focus, which autofocus (P34) can measure.

## Alternatives considered

- **Calibration in the projects' SQLite index.** It belongs to the instrument, not a project, and outlives projects: files beside them keep it apart, and readable.
- **One mutable calibration per objective.** No history, and no way back from a bad calibration.
- **Fitting a similarity,** a turn and one scale (Umeyama, 1991). The general matrix's aspect and skew show faults in a stage or a sensor that a similarity would hide.
- **Measuring backlash from the stage's own position reports.** A stepper stage reports where it was told to go. The images see where it is.
