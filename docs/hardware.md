# The instrument's hardware

What the real camera, SDK, stage controller, lights and host offer, so that the frame model, the HAL and the simulator fit them (D31). Written in P02b; M3's bring-up (H0) keeps it up to date.

**Where things stand:**

- **The camera** isn't connected to the host yet. Its facts come from the vendor's documentation, the installed SDK, and the owner's earlier test tool, run on another PC. What none of them answers is measured when the camera is connected, at H0 at the latest.
- **The stage controller** isn't connected either. OScope runs on the simulated stage until M3, and the controller's items are recorded at H0.
- **The lights** aren't chosen. They are simulated until they are.

Each fact gives its source:

- **doc:** the vendor's published documentation (listed under *Sources*);
- **SDK:** the TUCam SDK's own documentation and headers, as installed on the host;
- **tool:** measured with the owner's MIChrome 20 test tool, on an earlier PC with USB 3.1;
- **host:** read or measured on the host PC;
- **owner:** stated by the owner.

## Camera: Tucsen MIChrome 20

| Item | Value | Source |
|---|---|---|
| Sensor | Sony IMX183, colour CMOS, **rolling shutter**. 15.86 mm diagonal (1″); effective area 13.1 × 8.8 mm | doc [1] |
| Resolution | 5472 × 3648 (20 MP), 2.4 × 2.4 µm pixels | doc [1] |
| Interface | USB 3.0; C-mount; 3 W | doc [1] |
| Connectors | **USB only:** no trigger or other I/O | owner |
| Size and weight | 68 × 68 × 46 mm, 330 g | doc [1] |
| Pixel clock | 72 MHz | SDK [3] |
| Modes and frame rates | With 1 ms exposure, 8-bit:<ul><li>0: 5472 × 3648, 14.5 fps (about 870 MB/s from the SDK);</li><li>1: 2736 × 1824, 53.2 fps (about 800 MB/s);</li><li>2: 1824 × 1216, 67.3 fps (about 450 MB/s).</li></ul>In 16-bit colour, mode 0 gives only 7.3 fps. Software binning by summing (1–4) is offered too. The product page lists 2 × 2, 3 × 3 and 4 × 4 binning | tool [8], SDK [3], doc [1] |
| Frame rate and exposure | Under automatic exposure, the rate falls to 1 / exposure in a dim scene | tool [8] |
| ROI | The SDK has an ROI call; whether this camera accepts one is unknown | SDK [4]; when connected |
| Raw mosaic | One sample per pixel, **RGGB**, 16-bit, on request. **The SDK has already processed it:**<ul><li>gamma and contrast are applied. At gamma 128 and contrast 32 it is linear, to ±1% over an 8× range of exposure;</li><li>the white-balance gains are applied;</li><li>it is rescaled to fill 16 bits (about 58,000 levels), with a black offset of about 2,000.</li></ul>So it is not the ADC's 12-bit counts. **Its frame rate at full resolution is unmeasured** | tool [8], SDK [4] |
| Processed data | 8-bit, or 16-bit little-endian, BGR. Rows come bottom-up, because the SDK's Windows default flips them vertically | tool [8], SDK [3] |
| Exposure | 18.4 µs to 19,340 ms (the product page says 18 µs to 15 s). The range depends on the mode. **Automatic exposure is on by default** | tool [8], doc [1], SDK [3] |
| Gain | Analogue gain 0–240, default 30. Gains per colour channel are 0–1366, with 256 neutral. **Automatic white balance is on by default** | SDK [3], tool [8] |
| Image processing in the SDK | Mirroring and rotation. **A vertical flip is on by default on Windows.** Also flat-field and defect-pixel correction, HDR, levels, gamma, contrast, saturation, hue, sharpness, a monochrome mode, and 50/60 Hz anti-flicker | SDK [3] |
| Settings | The SDK remembers the last settings used, per camera, in a file under `%LOCALAPPDATA%`; a driver must set each one explicitly | tool [8] |
| Timestamps | **None from the camera.** The SDK's timestamp capability is listed for the MIChrome 20BW, not the 20 | SDK [3] |
| Frame numbers | **The SDK's frame index stays 0,** so dropped frames can't be found from it | tool [8] |
| Faults seen | **In mode 1, some frames arrive with the green channel missing,** wholly or in part: 0 to about 90 frames in 4 s, anywhere in a run. Modes 0 and 2 never showed it. There is also a faint periodic pattern (about 4 px), and occasional coloured dashed rows | tool [8] |
| Trigger input and output | **None** | owner; doc [1], SDK [3] |
| Buffers and callbacks | The SDK allocates frame buffers, or attaches the caller's. Frames are taken by a blocking wait with a timeout, or by a callback | SDK [4] |
| Cooling | None listed | doc [1] |
| Vendor software | Mosaic V3, with live stitching and live EDF, on Windows 10/11 (64-bit) and macOS | doc [1] |

**The rolling shutter** exposes the sensor's rows one after another.
- The readout takes about 60 ms at full resolution and about 15 ms in mode 2. That is from the test tool's tracking bands, each about 2.5 ms of readout: 24 at full resolution, 6 in mode 2.
- In a frame taken while the stage moves or the table shakes, each row sees a slightly different position, so the image is sheared, or bent.

**The camera that replaces it** will have an external trigger (owner). Its model isn't chosen.

## Camera SDK: TUCam

| Item | Value | Source |
|---|---|---|
| Version | TUCam 2.0.8.0. The kit is `TUCam_FV2.0.8.0_PV2.0.151.0`, and the camera driver (TUUSB3) is V2.1.6.1 | host, doc [1], tool [8] |
| Where it is | `C:\Program Files (x86)\TUCam_SDK`. The 64-bit runtime is `runtime\x64`: `TUCam.dll` and its dependencies, namely OpenCV 2.4.10 libraries, the Visual C++ 2013 runtime, and frame-grabber libraries. The installer copies only the 32-bit `TUCam.dll` into Windows, so a 64-bit program loads the runtime from that folder. Headers are in `library\inc` | host, tool [8] |
| Platforms | Windows and Linux kits | doc [2] |
| Languages | C and C++ (`TUCamApi.h`, `TUDefine.h`), C#, Python; DirectShow and TWAIN; Micro-Manager, MATLAB and LabVIEW | SDK, doc [1], [2] |
| Documentation | API and sample guides in English and Chinese, and a properties-and-capabilities document per camera series | SDK |
| Cameras at once | Up to 4, sharing the USB bandwidth | doc [1], SDK [3] |
| Licence and redistribution | **No licence file in the kit,** and none published. So users install the SDK themselves, and OScope loads it at run time (ADR-0002) | host, doc [2] |

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
| Camera trigger wiring | None for the MIChrome 20; an aux output, for the camera that replaces it | owner |

## Lights

**Not chosen yet** (owner). OScope uses the simulator's lights, through generic HAL traits: channels, on/off and intensity (P09). The real driver comes in M3 (H3), once the lights and their controller are chosen.

## Host

The target host is a PC like this one, in CPU, GPU, RAM and SSD (owner). These values were read on this PC.

| Item | Value | Source |
|---|---|---|
| CPU | Intel Core i9-12900: 16 cores (8 performance, 8 efficient), 24 threads | host |
| RAM | 64 GB | host |
| GPU | NVIDIA RTX A2000 12 GB: CUDA compute capability 8.6, 12,282 MiB, PCIe gen 4 × 16, driver 596.51 (CUDA 13.2). Also Intel UHD Graphics 770. The CUDA 13.4 toolkit was installed for P07: a 40 MB upload from pinned memory takes 1.64 ms (24.4 GB/s) | host |
| USB | Intel USB 3.20 and USB 3.10 xHCI host controllers | host |
| Disk | One NVMe SSD, Micron MTFDKBA1T0TFH, 1 TB (firmware 7006P7LN), which is also the system disk: 528 GB free | host |
| Sustained write | Needed: about 580 MB/s for full-rate raw recording (§5.5). Measured on 2026-11-12 by writing 150 GB in 64 MB blocks of random data, flushed every 1 GB. The rate was **1.9 GB/s for the first ~55 GB**, the drive's fast cache by the look of it. **From then on it was 1.10–1.17 GB/s.** Overall 1,332 MB/s, and the slowest 10 GB ran at 1,100 MB/s. That is about **1.9 × the recording rate**, sustained, with the system on the same disk | host |
| Operating system | Windows 11 Pro, build 26300 | host |

## The environment

**The test table shook at about 49 Hz.** At full resolution, frame-to-frame jitter was 8.1 px horizontally and 5.9 px vertically (RMS), from something on the 50 Hz mains nearby (tool [8]). The rolling shutter turns fast shake into bending within a frame. That table may not be the instrument's final one, but autofocus, live stitching and long exposures will feel any shake on the final table too.

## Consequences for the plan

Applied in P02b's pull request, with a line in the plan's change log.

- **The rolling shutter.**
  - The plan's camera and §5.5's frame metadata now carry the line time.
  - P12's simulated camera exposes its rows one after another, and tests the shear.
  - M3's continuous scanning allows for it.
- **No camera timestamps.**
  - Frames are timed on the host. A sensor timestamp is optional in the frame metadata (§5.5, P03), and P12 simulates host timestamps only.
  - Because the SDK's frame index stays 0, M3's driver finds dropped frames from their timing.
- **No trigger on the MIChrome 20, but on its successor.**
  - The HAL's camera keeps external triggering, as a capability (§5.9, P09).
  - P11's trigger output and P12's trigger input stand in for the future camera.
  - D20 and M3's continuous scanning (H5) say which camera each path needs.
- **The raw mosaic is processed.**
  - M3's driver sets gamma 128, contrast 32 and fixed white-balance gains to keep it linear, and P08 subtracts its black level.
  - P12 simulates the same rescaled 16-bit output, and the frame metadata carries the black level.
- **The SDK's defaults** (automatic exposure, automatic white balance, a vertical flip on Windows, remembered settings) are set explicitly by M3's driver (H2), which also detects mode 1's broken frames.
- **The lights aren't chosen,** so §5.9's `Illumination` and P09 stay generic, and the simulator provides them.
- **Still open:**
  - the raw mosaic's frame rate at full resolution, which decides whether 580 MB/s (§5.5, §5.20) holds;
  - whether the camera accepts an ROI;
  - the controller's items, at H0.

## Sources

1. Tucsen, MIChrome 20 product page and specifications: <https://www.tucsen.com/michrome-20-product/>, and <https://tucsen.com/Home/Product/parameter/dataid/25/id/32.html>.
2. Tucsen, SDK support: <https://tucsen.com/sdk-software-support-product>.
3. Tucsen, *MIChrome Series TUCAM-API Properties & Capabilities* (2023), in the SDK kit: `documents\EN\Capabilities&Properties`.
4. Tucsen, TUCam SDK 2.0.8.0 headers, `TUCamApi.h` and `TUDefine.h`, in the SDK kit: `library\inc`.
5. grblHAL core wiki, *Report extensions*: <https://github.com/grblHAL/core/wiki/Report-extensions>.
6. grblHAL core wiki, *Additional G- and M-codes*: <https://github.com/grblHAL/core/wiki/Additional-G--and-M-codes>.
7. grblHAL core wiki, *Additional or extended settings*: <https://github.com/grblHAL/core/wiki/Additional-or-extended-settings>.
8. The owner's MIChrome 20 test tool (`michrome`), and the measurements in its README, taken on an earlier PC with USB 3.1. It is kept outside the repository.
