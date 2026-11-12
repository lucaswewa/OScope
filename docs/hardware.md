# The instrument's hardware

What the real camera, SDK, stage controller, lights and host offer, so that the frame model, the HAL and the simulator fit them (D31). Written in P02b; M3's bring-up (H0) keeps it up to date.

> **Draft.** The vendors' documentation, the installed SDK and the host are in. What needs the camera attached, or the owner's eyes on the instrument, is marked **to measure** or **to check**; the [P02b notes](milestone-1/notes/P02b-hardware-survey.md) say how. The stage controller isn't connected to the host yet, so OScope uses the simulated stage until M3, and the controller's items wait for M3's bring-up.

Each fact gives its source:

- **doc:** the vendor's published documentation (listed under *Sources*);
- **SDK:** the TUCam SDK's own documentation and headers, as installed on the host;
- **host:** read or measured on the host PC;
- **instrument:** measured on the instrument.

## Camera: Tucsen MIChrome 20

| Item | Value | Source |
|---|---|---|
| Sensor | Sony IMX183, colour CMOS, **rolling shutter**. 15.86 mm diagonal (1″); effective area 13.1 × 8.8 mm | doc [1] |
| Resolution | 5472 × 3648 (20 MP), 2.4 × 2.4 µm pixels | doc [1] |
| Interface | USB 3.0; C-mount; 3 W | doc [1] |
| Size and weight | 68 × 68 × 46 mm, 330 g | doc [1] |
| Pixel clock | 72 MHz | SDK [3] |
| Modes | Three resolutions: 5472 × 3648, 2736 × 1824 and 1824 × 1216. The SDK doesn't say whether the two smaller ones bin or resample. Software binning by summing (1–4) is offered too. The product page lists 2 × 2, 3 × 3 and 4 × 4 binning | SDK [3], doc [1] |
| Frame rates | 14.5 fps at full resolution; the other modes' rates aren't stated | doc [1]; **to measure** |
| ROI | The SDK has an ROI call; whether this camera accepts one isn't stated | SDK [4]; **to measure** |
| Bit depth | 8- or 16-bit data, 8 by default. The ADC's depth, and where its bits sit in a 16-bit word, aren't stated. The SDK can show any 8 consecutive bits of the 16 | SDK [3]; **to measure** |
| Raw data | The SDK asks for a frame as raw, "usual" or RGB888 data. Whether raw is the Bayer mosaic, and its pattern, isn't stated | SDK [4]; **to measure** |
| Exposure | 18 µs to 15 s. The SDK gives the range for the current mode at run time. **Automatic exposure is on by default** | doc [1], SDK [3] |
| Gain | A global gain of 0–240, default 30, its unit not stated. Gains per colour channel are 0–1366, default 256. **Automatic white balance is on by default** | SDK [3] |
| Image processing in the SDK | Mirroring and rotation. **A vertical flip is on by default on Windows.** Also flat-field and defect-pixel correction, HDR, levels, gamma, contrast, saturation, hue, sharpness, a monochrome mode, and 50/60 Hz anti-flicker | SDK [3] |
| Timestamps | **None from the camera:** the SDK's timestamp capability is listed for the MIChrome 20BW, not the 20. Frames carry a time field; it is presumably the host's, **to check** | SDK [3], [4] |
| Trigger input and output | **Probably none.** Neither the product pages nor the SDK's MIChrome document mention one. The SDK has trigger calls, which other Tucsen series use | doc [1], SDK [3], [4]; **to check** on the camera body |
| Buffers and callbacks | The SDK allocates frame buffers, or attaches the caller's. Frames are taken by a blocking wait with a timeout, or by a callback | SDK [4] |
| Cooling | None listed | doc [1] |
| Vendor software | Mosaic V3, with live stitching and live EDF, on Windows 10/11 (64-bit) and macOS | doc [1] |

**The rolling shutter** exposes the sensor's rows one after another.
- If readout limits the frame rate, a row starts about 1 / (14.5 × 3648) ≈ 19 µs after the one above it, and the last row starts about 69 ms after the first. The line time is **to measure**.
- In a frame taken while the stage moves, each row sees a slightly different position, and the image is sheared along the motion.

## Camera SDK: TUCam

| Item | Value | Source |
|---|---|---|
| Version | TUCam 2.0.8.0. The kit is `TUCam_FV2.0.8.0_PV2.0.151.0`, and the camera driver is V2.1.6.1 | host, doc [1] |
| Where it is | `C:\Program Files (x86)\TUCam_SDK`. The 64-bit runtime is `runtime\x64`, with `TUCam.dll` and its dependencies: OpenCV 2.4.10 libraries, the Visual C++ 2013 runtime, and frame-grabber libraries. The headers are `library\inc`, and the import libraries `library\lib` | host |
| Platforms | Windows and Linux kits | doc [2] |
| Languages | C and C++ (`TUCamApi.h`, `TUDefine.h`), C#, Python; DirectShow and TWAIN; Micro-Manager, MATLAB and LabVIEW | SDK, doc [1], [2] |
| Documentation | API and sample guides in English and Chinese, and a properties-and-capabilities document per camera series | SDK |
| Cameras at once | Up to 4, sharing the USB bandwidth | doc [1], SDK [3] |
| Licence and redistribution | **No licence file in the kit**, and none published. So users install the SDK themselves, and OScope loads it at run time (ADR-0002) | host, doc [2] |

## Stage controller: grblHAL

**The controller isn't connected to the host yet.** Until M3, OScope runs on the simulated stage (P11), which behaves like grblHAL. Every item below is recorded at M3's bring-up (H0), from the commands in the P02b notes.

| Item | Value | Source |
|---|---|---|
| Board, firmware and build options | `$I` and `$I+` report them: `[OPT:…]`, `[AXS:…]`, `[NEWOPT:…]`, `[FIRMWARE:grblHAL]`, `[AUX IO:…]`, `[DRIVER:…]`, `[BOARD:…]`, `[PLUGIN:…]` | format: doc [5]; values: unknown until H0 |
| Each axis | Travel, steps per mm, maximum rate and acceleration (`$$`: `$100`–`$132`) | unknown until H0 |
| Homing and limits | Hard limits (`$21`), homing options (`$22`), and the switches fitted | unknown until H0 |
| Status report | `?` gives `<state|MPos:…|FS:…|Pn:…|…>`. `$10` chooses its optional fields, and some are sent only when they change. `0x87` asks for a full report | doc [5], [7] |
| Status-report rate | Not stated | unknown until H0 |
| Encoders | grblHAL's core status report has **no encoder field**. How the linear encoders are read (a plugin, the controller, or a separate readout) is unknown | doc [5]; unknown until H0 |
| Aux outputs | `M62`–`M65` are "digital output control" where the driver supports them; `[AUX IO:…]` in `$I+` counts the ports | doc [6]; count unknown until H0 |
| Camera trigger wiring | Moot if the camera has no trigger input (above) | **to check** |

## Lights

| Item | Value | Source |
|---|---|---|
| Fitted | Transmitted brightfield and reflected (coaxial), per the plan | **to confirm** |
| Switching and dimming today | Unknown | **to check** |

## Host

The target host is a PC like this one, in CPU, GPU, RAM and SSD. These values were read on this PC.

| Item | Value | Source |
|---|---|---|
| CPU | Intel Core i9-12900: 16 cores (8 performance, 8 efficient), 24 threads | host |
| RAM | 64 GB | host |
| GPU | NVIDIA RTX A2000 12 GB: CUDA compute capability 8.6, 12,282 MiB, PCIe gen 4 × 16, driver 596.51. Also Intel UHD Graphics 770 | host |
| USB | Intel USB 3.20 and USB 3.10 xHCI host controllers. The port the camera uses is **to note** | host |
| Disk | One NVMe SSD, Micron MTFDKBA1T0TFH, 1 TB (firmware 7006P7LN), which is also the system disk: 528 GB free | host |
| Sustained write | Needed: about 580 MB/s for full-rate recording (§5.5). Writing far past the drive's cache finds its sustained rate | **to measure** |
| Operating system | Windows 11 Pro, build 26300 | host |

## Consequences for the plan

These are the findings so far that may change the plan. The P02b pull request updates the affected specs, and adds a line to the plan's change log.

- **The rolling shutter is not in the plan.**
  - **P03:** the frame metadata needs the line time, to place each row's exposure in time.
  - **P12:** the simulated camera should expose its rows one after another.
  - **P38, live stitching:** frames taken while moving are sheared.
  - **M3, continuous scanning:** the stage's speed must allow for the 69 ms from the first row to the last.
- **The camera gives no timestamps of its own.**
  - Frames are timed when they reach the host, less an estimated latency, as §5.5 already does for the stage's reports.
  - P03 keeps a sensor timestamp optional, and P12 simulates the host's arrival time.
- **A hardware trigger probably doesn't exist.**
  - The plan assumes one: D20, P11 and P12, and M3's continuous scanning.
  - Without one, M3's on-the-fly scans rely on host timestamps and the position track, or on stop-and-go.
  - P11's trigger output and P12's trigger input remain useful for testing that logic, but nothing on this instrument would use them.
- **The SDK's defaults** are automatic exposure, automatic white balance and a vertical flip on Windows. M3's Tucsen driver must set each of them explicitly, so that frames mean the same on every platform (ADR-0007's image coordinates).
- **Encoder positions** may not reach OScope through grblHAL's status report. That affects P11's "true versus commanded position", and M3's grblHAL driver. It is unknown until H0.
- **The scratch disk is the system disk,** on a host like this one. If its sustained write rate is below 580 MB/s, or if recording should not share the system disk, full-rate recording needs a second NVMe.

## Sources

1. Tucsen, MIChrome 20 product page and specifications: <https://www.tucsen.com/michrome-20-product/>, and <https://tucsen.com/Home/Product/parameter/dataid/25/id/32.html>.
2. Tucsen, SDK support: <https://tucsen.com/sdk-software-support-product>.
3. Tucsen, *MIChrome Series TUCAM-API Properties & Capabilities* (2023), in the SDK kit: `documents\EN\Capabilities&Properties`.
4. Tucsen, TUCam SDK 2.0.8.0 headers, `TUCamApi.h` and `TUDefine.h`, in the SDK kit: `library\inc`.
5. grblHAL core wiki, *Report extensions*: <https://github.com/grblHAL/core/wiki/Report-extensions>.
6. grblHAL core wiki, *Additional G- and M-codes*: <https://github.com/grblHAL/core/wiki/Additional-G--and-M-codes>.
7. grblHAL core wiki, *Additional or extended settings*: <https://github.com/grblHAL/core/wiki/Additional-or-extended-settings>.
