# The instrument's hardware

What the real camera, SDK, stage controller, lights and host offer, so that the frame model, the HAL and the simulator fit them (D31). Written in P02b; M3's bring-up (H0) keeps it up to date.

**Where things stand:**

- **The camera** was connected to the host on 2026-11-12, mounted on the microscope, and measured with the owner's test tool: its modes, frame rates, ROI, and a 10-minute run at full rate (*Measured on the host*, below). Its other facts come from the vendor's documentation, the installed SDK, and the test tool's earlier measurements on another PC.
- **The stage controller** isn't connected either. OScope runs on the simulated stage until M3, and the controller's items are recorded at H0.
- **The lights** aren't chosen. They are simulated until they are.

Each fact gives its source:

- **doc:** the vendor's published documentation (listed under *Sources*);
- **SDK:** the TUCam SDK's own documentation and headers, as installed on the host;
- **tool:** measured with the owner's MIChrome 20 test tool, on an earlier PC with USB 3.1;
- **measured:** measured with the same tool on the host, with the camera on the microscope, on 2026-11-12;
- **host:** read or measured on the host PC;
- **owner:** stated by the owner.

## Camera: Tucsen MIChrome 20

| Item | Value | Source |
|---|---|---|
| Sensor | Sony IMX183, colour CMOS, **rolling shutter**. 15.86 mm diagonal (1″); effective area 13.1 × 8.8 mm | doc [1] |
| Resolution | 5472 × 3648 (20 MP), 2.4 × 2.4 µm pixels | doc [1] |
| Interface | USB 3.0; C-mount; 3 W. On the host it is on a SuperSpeed port of the Intel USB 3.20 controller (port 17), with the TUUSB3 driver 2.1.6.1. The SDK reports the bus as USB 3.1 | doc [1], host, measured |
| Connectors | **USB only:** no trigger or other I/O | owner |
| Size and weight | 68 × 68 × 46 mm, 330 g | doc [1] |
| Pixel clock | 72 MHz | SDK [3] |
| Modes and frame rates | With 1 ms exposure, in 8 bits, then 16:<ul><li>0: 5472 × 3648, **14.49 fps, then 7.24**;</li><li>1: 2736 × 1824, 53.14 fps, then 26.57;</li><li>2: 1824 × 1216, 67.26 fps, then 33.87.</li></ul>**16 bits halve the rate in every mode,** raw or colour: the camera delivers about 289 MB/s of sensor data at full resolution, whatever the format (*Measured on the host*). Software binning by summing (1–4) is offered too. The product page lists 2 × 2, 3 × 3 and 4 × 4 binning | measured, tool [8], SDK [3], doc [1] |
| Frame rate and exposure | Under automatic exposure, the rate falls to 1 / exposure in a dim scene | tool [8] |
| ROI | **Accepted, but it doesn't raise the frame rate:** every ROI tried ran at the full frame's rate, and only the data delivered shrank. The camera rounds the ROI's x offset and width down to even numbers, but keeps an odd y offset and height. An odd y offset turns the mosaic's first cell from RGGB to GBRG | measured |
| Raw mosaic | One sample per pixel, **RGGB**, 8 or 16 bits, on request. **The SDK has already processed it:**<ul><li>gamma and contrast are applied. At gamma 128 and contrast 32 it is linear, to ±1% over an 8× range of exposure;</li><li>the white-balance gains are applied;</li><li>it is rescaled to fill 16 bits (about 58,000 levels), with a black offset of about 2,000.</li></ul>So it is not the ADC's 12-bit counts. **At full resolution it runs at 7.24 fps in 16 bits (289 MB/s),** and at 14.49 fps in 8 bits | measured, tool [8], SDK [4] |
| Processed data | 8-bit, or 16-bit little-endian, BGR. Rows come bottom-up, because the SDK's Windows default flips them vertically | tool [8], SDK [3] |
| Exposure | 18.4 µs to 19,340 ms (the product page says 18 µs to 15 s). The range depends on the mode. **Automatic exposure is on by default** | tool [8], doc [1], SDK [3] |
| Gain | Analogue gain 0–240, default 30. Gains per colour channel are 0–1366, with 256 neutral. **Automatic white balance is on by default** | SDK [3], tool [8] |
| Image processing in the SDK | Mirroring and rotation. **A vertical flip is on by default on Windows.** Also flat-field and defect-pixel correction, HDR, levels, gamma, contrast, saturation, hue, sharpness, a monochrome mode, and 50/60 Hz anti-flicker | SDK [3] |
| Settings | The SDK remembers the last settings used, per camera, in a file under `%LOCALAPPDATA%`; a driver must set each one explicitly | tool [8] |
| Timestamps | **None from the camera.** The SDK's timestamp capability is listed for the MIChrome 20BW, not the 20 | SDK [3] |
| Frame numbers | **The SDK's frame index stays 0,** so dropped frames can't be found from it | tool [8] |
| Faults seen | **In mode 1, at 8 bits, some frames arrive with the green channel missing,** wholly or in part: about one in five in 30 s on the host, in the raw mosaic as well as in colour. At 16 bits, and in modes 0 and 2, it never showed. There is also a faint periodic pattern (about 4 px), and occasional coloured dashed rows | measured, tool [8] |
| Trigger input and output | **None** | owner; doc [1], SDK [3] |
| Buffers and callbacks | The SDK allocates frame buffers (4 by default), or attaches the caller's. Frames are taken by a blocking wait with a timeout, or by a callback | SDK [4], measured |
| Cooling | None listed | doc [1] |
| Vendor software | Mosaic V3, with live stitching and live EDF, on Windows 10/11 (64-bit) and macOS | doc [1] |

**The rolling shutter** exposes the sensor's rows one after another.
- The readout takes about 60 ms at full resolution and about 15 ms in mode 2. That is from the test tool's tracking bands, each about 2.5 ms of readout: 24 at full resolution, 6 in mode 2.
- In a frame taken while the stage moves or the table shakes, each row sees a slightly different position, so the image is sheared, or bent.

**The camera that replaces it** will have an external trigger (owner). Its model isn't chosen.

### Measured on the host

On 2026-11-12, with the camera on the microscope under its brightfield light, and the owner's test tool. Its `bench` command was extended for this, outside the repository, to:

- ask for the raw mosaic or the colour picture, in 8 or 16 bits;
- set an ROI, and read it back;
- copy each frame out of the SDK's buffer, as a driver would;
- report the frame intervals' median, 99th percentile and maximum, and the **gaps**: intervals over 1.5 × the median. With the frame index stuck at 0, gaps are how missed frames show;
- count the **broken** frames:
  - in colour, a green level below 60% of the last good frame's;
  - in the raw mosaic, one diagonal pair of the 2 × 2 cell against the other, off by 40% from the last good frame's in either direction;
  - a frame of the wrong size.

Each run used a 1 ms exposure, automatic exposure off, and lasted 30 s, unless the table says otherwise. No frame timed out in any run.

| Output | Mode | Frames | Rate | Data | Interval: median, p99, max | Gaps | Broken |
|---|---|---|---|---|---|---|---|
| Raw, 16-bit | 0: 5472 × 3648 | 218 | 7.24 fps | 289 MB/s | 138, 149, 151 ms | 0 | 0 |
| | 1: 2736 × 1824 | 798 | 26.57 fps | 265 MB/s | 37.6, 38.1, 38.5 ms | 0 | 0 |
| | 2: 1824 × 1216 | 1,017 | 33.87 fps | 150 MB/s | 29.5, 36.4, 43.9 ms | 0 | 0 |
| Raw, 8-bit | 0 | 435 | 14.49 fps | 289 MB/s | 69.1, 70.6, 71.1 ms | 0 | 0 |
| | 1 | 1,595 | 53.14 fps | 265 MB/s | 18.8, 19.7, 33.8 ms | 2 | **281** |
| | 2 | 2,018 | 67.26 fps | 149 MB/s | 14.9, 16.0, 16.5 ms | 0 | 0 |
| Colour, 16-bit | 0 | 218 | 7.24 fps | 868 MB/s | 138, 141, 144 ms | 0 | 0 |
| | 1 | 798 | 26.57 fps | 796 MB/s | 37.6, 39.4, 53.2 ms | 0 | 0 |
| | 2 | 1,017 | 33.87 fps | 451 MB/s | 29.5, 30.6, 40.9 ms | 0 | 0 |
| Colour, 8-bit | 0 | 434 | 14.46 fps | 866 MB/s | 69.0, 79.3, 129 ms | 1 | 0 |
| | 1 | 1,595 | 53.14 fps | 796 MB/s | 18.8, 19.4, 25.9 ms | 0 | **319** |
| | 2 | 2,018 | 67.26 fps | 448 MB/s | 14.9, 17.0, 38.6 ms | 5 | 0 |
| **Raw, 16-bit, 10 minutes,** each frame copied out | 0 | 4,346 | **7.24 fps** in every minute | 289 MB/s | 138.1, 147.6, 159.5 ms | **0** | **0** |

- **The colour output** is the SDK's BGR, three samples per pixel, so its data rate is three times the raw one at the same frame rate.
- **The first run of the day, 8-bit colour in mode 0,** gave 10.89 fps with 108 gaps: about one frame in four was missed, its intervals 69 ms or 138 ms. Repeated, it gave the row above. Nothing else missed frames like that.
- **The other gaps** are single frames among hundreds or thousands: late ones of up to 39 ms in modes 1 and 2, and in mode 0's 8-bit colour one interval of 129 ms, a missed frame. An earlier run of raw 8-bit in modes 0 and 2, before the raw check existed, had the same rates, with 6 gaps in mode 2.

**ROIs,** each in mode 0, raw 16-bit, for 20 s:

| Asked | Read back | Rate | Data |
|---|---|---|---|
| 2736 × 1824 at (1368, 912) | the same | 7.24 fps | 72.3 MB/s |
| 1024 × 1024 at (2224, 1312) | the same | 7.24 fps | 15.2 MB/s |
| 512 × 512 at (2480, 1568) | the same | 7.24 fps | 3.8 MB/s |
| 1001 × 1001 at (333, 333) | **1000 × 1001 at (332, 333)** | 7.24 fps | 14.5 MB/s |
| 1000 × 1000 at (332, 332) | the same | 7.24 fps | 14.5 MB/s |

The odd ROI's first run counted 2 broken frames, and its second none. The camera's ROI was off again after each run.

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
| Sustained write | Needed: 580 MB/s, §5.20's recording budget (§5.5). The MIChrome 20 itself delivers 289 MB/s. Measured on 2026-11-12 by writing 150 GB in 64 MB blocks of random data, flushed every 1 GB. The rate was **1.9 GB/s for the first ~55 GB**, the drive's fast cache by the look of it. **From then on it was 1.10–1.17 GB/s.** Overall 1,332 MB/s, and the slowest 10 GB ran at 1,100 MB/s. That is about **1.9 × the budget**, sustained, with the system on the same disk | host |
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

From the measurements of 2026-11-12, applied in their own pull request with a line in the plan's change log:

- **The camera delivers 289 MB/s, half of the 580 MB/s that §5.5 assumed.** Full resolution runs at 14.5 fps only in 8 bits; in 16 bits it runs at 7.24 fps.
  - At the owner's word, **580 MB/s stays the design budget** (§5.5, §5.20, P29): headroom for the camera that replaces the MIChrome 20.
  - P12's simulated camera runs its modes at the measured rates, for each bit depth. It also has a design rate, 16 bits at 14.5 fps at full resolution, so that P29 can record at 580 MB/s.
  - Whether to record full resolution at 8 bits and 14.5 fps, or at 16 bits and 7.24 fps, is M3's driver's choice (H2).
- **An ROI doesn't raise the frame rate.** It only shrinks the frames, so preview and focus ROIs save host work, not camera time. P12's ROIs do the same. The driver keeps an ROI's offsets even, so that the mosaic stays RGGB; ADR-0008's rectangles carry their pattern anyway.
- **Mode 1's broken frames come at 8 bits only,** in the raw mosaic too. M3's driver detects them in raw frames, or avoids mode 1 at 8 bits.
- **Still open:** the controller's items, at H0.

## Sources

1. Tucsen, MIChrome 20 product page and specifications: <https://www.tucsen.com/michrome-20-product/>, and <https://tucsen.com/Home/Product/parameter/dataid/25/id/32.html>.
2. Tucsen, SDK support: <https://tucsen.com/sdk-software-support-product>.
3. Tucsen, *MIChrome Series TUCAM-API Properties & Capabilities* (2023), in the SDK kit: `documents\EN\Capabilities&Properties`.
4. Tucsen, TUCam SDK 2.0.8.0 headers, `TUCamApi.h` and `TUDefine.h`, in the SDK kit: `library\inc`.
5. grblHAL core wiki, *Report extensions*: <https://github.com/grblHAL/core/wiki/Report-extensions>.
6. grblHAL core wiki, *Additional G- and M-codes*: <https://github.com/grblHAL/core/wiki/Additional-G--and-M-codes>.
7. grblHAL core wiki, *Additional or extended settings*: <https://github.com/grblHAL/core/wiki/Additional-or-extended-settings>.
8. The owner's MIChrome 20 test tool (`michrome`): the measurements in its README, taken on an earlier PC with USB 3.1, and those of 2026-11-12 on the host, with its `bench` command extended as *Measured on the host* describes. It is kept outside the repository.
