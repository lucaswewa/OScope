# The instrument's hardware

What the real camera, SDK, stage controller, lights and host offer, so that the frame model, the HAL and the simulator fit them (D31). Written in P02b; M3's bring-up (H0) keeps it up to date.

> **Draft.** The vendors' documentation and the host are in. The measurements on the instrument are still to come; each is marked **to measure**, and the [P02b notes](milestone-1/notes/P02b-hardware-survey.md) say how to take it.

Each fact gives its source:

- **doc**, the vendor's published documentation (linked below);
- **host**, read or measured on the host PC;
- **SDK**, the SDK's own documentation or samples;
- **instrument**, measured on the instrument.

## Camera: Tucsen MIChrome 20

| Item | Value | Source |
|---|---|---|
| Sensor | Sony IMX183, colour CMOS, **rolling shutter**. 15.86 mm diagonal (1″); effective area 13.1 × 8.8 mm | doc [1] |
| Resolution | 5472 × 3648 (20 MP), 2.4 × 2.4 µm pixels | doc [1] |
| Interface | USB 3.0; C-mount; 3 W | doc [1] |
| Size and weight | 68 × 68 × 46 mm, 330 g | doc [1] |
| Frame rate, full resolution | 14.5 fps | doc [1]; **to measure** |
| Binning | 2 × 2, 3 × 3, 4 × 4 are listed; their frame rates aren't | doc [1]; rates **to measure** |
| ROI | Not stated | **to measure** (SDK) |
| Exposure | 18 µs to 15 s | doc [1] |
| Gain | Not stated | **to measure** (SDK) |
| Bit depth | "16/8 bit" output. The ADC's depth, and the alignment of its bits in a 16-bit word, aren't stated | doc [1]; **to measure** |
| Raw Bayer data | Not stated, nor the Bayer pattern | **to measure** (SDK) |
| White balance | A colour temperature of 2000–15000 K | doc [1] |
| Timestamps | Not stated: their source (sensor or host) and resolution | **to measure** (SDK) |
| Trigger input and output | **None listed** on the product pages | **to check** on the camera body and in the SDK |
| Buffers and callbacks | Not stated | SDK |
| Cooling | None listed | doc [1] |
| Vendor software | Mosaic V3, with live stitching and live EDF, on Windows 10/11 (64-bit) and macOS | doc [1] |

**The rolling shutter** exposes the sensor's rows one after another.
- If readout limits the frame rate, a row starts about 1 / (14.5 × 3648) ≈ 19 µs after the one above it, so the last row starts about 69 ms after the first. The line time is still **to measure**.
- In a frame taken while the stage moves, each row therefore sees a slightly different position, and the image is sheared along the motion.

## Camera SDK: TUCam

| Item | Value | Source |
|---|---|---|
| Name and version | The "Tucsen SDK Kit for Windows" download is `TUCam_FV2.0.8.0_PV2.0.151.0-Setup.zip`; the camera driver is "TUCam Camera Driver" V2.1.6.1. A Linux kit is listed too | doc [1], [2] |
| Platforms | Windows and Linux | doc [2] |
| Languages | C, C++ and C#; DirectShow and TWAIN; Micro-Manager, MATLAB and LabVIEW | doc [1], [2] |
| Cameras at once | Up to 4 | doc [1] |
| Contents | Dynamic libraries and source code ("full SDK dynamic library files and source code") | doc [2] |
| Licence and redistribution | **Not published.** Until the kit's own licence is read, users install the SDK themselves (ADR-0002) | **to check** in the kit |

## Stage controller: grblHAL

| Item | Value | Source |
|---|---|---|
| Board, firmware and build options | `$I` and `$I+`: `[OPT:…]`, `[AXS:…]`, `[NEWOPT:…]`, `[FIRMWARE:grblHAL]`, `[AUX IO:…]`, `[DRIVER:…]`, `[BOARD:…]`, `[PLUGIN:…]` | format: doc [3]; values **to measure** |
| Each axis | Travel, steps per mm, maximum rate and acceleration (`$$`: `$100`–`$132`) | **to measure** |
| Homing and limits | Hard limits (`$21`), homing options (`$22`), and the switches fitted | **to measure** |
| Status report | `?` gives `<state|MPos:…|FS:…|Pn:…|…>`. `$10` chooses the optional fields; some are sent only when they change. `0x87` asks for a full report | doc [3], [5] |
| Status-report rate | Not stated. The rate achievable by polling `?` | **to measure** |
| Encoders | grblHAL's core status report has **no encoder field**. How the linear encoders are read — a plugin, the controller, or a separate readout — is unknown | doc [3]; **to check** |
| Aux outputs | `M62`–`M65` are "digital output control" when the driver supports them; `[AUX IO:…]` in `$I+` counts the ports | doc [4]; count **to measure** |
| Camera trigger wiring | Which output, to which input on the camera, if the camera has one (above) | **to check** |

## Lights

| Item | Value | Source |
|---|---|---|
| Fitted | Transmitted brightfield and reflected (coaxial), per the plan | **to confirm** |
| Switching and dimming today | Unknown | **to check** |

## Host

The camera and the controller are to be connected to this PC.

| Item | Value | Source |
|---|---|---|
| CPU | Intel Core i9-12900: 16 cores (8 performance, 8 efficient), 24 threads | host |
| RAM | 64 GB | host |
| GPU | NVIDIA RTX A2000 12 GB: CUDA compute capability 8.6, 12,282 MiB, PCIe gen 4 × 16, driver 596.51. Also Intel UHD Graphics 770 | host |
| USB | Intel USB 3.20 and USB 3.10 xHCI host controllers. The port the camera uses is **to note** | host |
| Disk | One NVMe SSD, Micron MTFDKBA1T0TFH, 1 TB (firmware 7006P7LN), which is also the system disk: 528 GB free | host |
| Sustained write | Needed: about 580 MB/s for full-rate recording (§5.5). Writing far past the drive's cache, to find its sustained rate | **to measure** |
| Operating system | Windows 11 Pro, build 26300 | host |

## Consequences for the plan

The findings so far that may change the plan. Once the measurements are in, P02b's pull request updates the affected specs and adds a line to the plan's change log.

- **The rolling shutter is not in the plan.**
  - **P03:** the frame metadata needs the line time, so that a row's exposure can be placed in time.
  - **P12:** the simulated camera should expose its rows one after another.
  - **P38, live stitching:** frames taken while moving are sheared.
  - **M3, continuous scanning:** the stage's speed during an exposure has to allow for the 69 ms from first row to last.
- **A hardware trigger may not exist.** The plan assumes one (D20, P11, P12, M3's continuous scanning), and the vendor lists none. If the camera has none, M3's on-the-fly scans fall back to timestamps and position tracks, or stop and go.
- **Encoder positions** may not reach OScope through grblHAL's status report. That affects P11's "true versus commanded position", and M3's grblHAL driver.
- **The scratch disk is the system disk.** If its sustained write rate is below 580 MB/s, or if recording should not share the system disk, a second NVMe is needed for full-rate recording.

## Sources

1. Tucsen, MIChrome 20 product page and specifications: <https://www.tucsen.com/michrome-20-product/>, and <https://tucsen.com/Home/Product/parameter/dataid/25/id/32.html>.
2. Tucsen, SDK support: <https://tucsen.com/sdk-software-support-product>.
3. grblHAL core wiki, *Report extensions*: <https://github.com/grblHAL/core/wiki/Report-extensions>.
4. grblHAL core wiki, *Additional G- and M-codes*: <https://github.com/grblHAL/core/wiki/Additional-G--and-M-codes>.
5. grblHAL core wiki, *Additional or extended settings*: <https://github.com/grblHAL/core/wiki/Additional-or-extended-settings>.
