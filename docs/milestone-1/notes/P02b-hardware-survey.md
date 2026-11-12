# P02b: Hardware capability survey (docs only)

- Status: In progress: the documentation, the installed SDK and the host are surveyed. The camera's measurements wait for it to be attached
- Branch / PR: `m1/p02b-hardware-survey` / not opened yet
- Size: S, docs only, so it isn't counted

## Summary

[`docs/hardware.md`](../../hardware.md) records what the real camera, SDK, stage controller, lights and host offer ([spec](../phases.md#p02b)). So far it holds:

- **the vendors' documentation;**
- **the TUCam SDK,** installed on the host, with its documents and headers;
- **the host:** a PC like the target one, as the owner said.

**The stage controller isn't connected to the host.** At the owner's word, OScope runs on the simulated stage until M3, and the controller's items are recorded at M3's bring-up (H0). The pull request opens once the camera's measurements are in.

**Findings that change the plan:**

- **The camera has a rolling shutter,** which the plan doesn't mention.
- **It gives no timestamps of its own:** frames are timed on the host.
- **It probably has no hardware trigger,** which the plan assumes.
- **The SDK starts with automatic exposure, automatic white balance and a vertical flip on Windows,** so the driver must set each explicitly.

## What was built

- `docs/hardware.md`: camera, SDK, stage controller, lights, host, the consequences for the plan, and the sources.
- `docs/README.md`: its row links the new page.

## How to try it

How to take, or repeat, each measurement.

**The camera, attached to the host.** These need no code in OScope. A short script against the installed SDK (`runtime\x64\TUCam.dll`), or the SDK's own samples, is enough; such a script isn't part of OScope.

1. **Modes and frame rates.** With a 1 ms exposure, stream for 10 s in each mode: 5472 × 3648, 2736 × 1824 and 1824 × 1216. Then try software binning, and a small ROI. Record:
   - each frame rate;
   - whether the ROI is accepted;
   - whether the smaller modes bin, or resample.
2. **Raw data.** Ask for a raw frame (the SDK's raw format), 16-bit, of a bright, even field. Record:
   - its size and channels: one channel means a Bayer mosaic;
   - the Bayer pattern, from the colours of a red and a green target;
   - whether the values are all multiples of 16 (12 significant bits, aligned high), of 4 (14 bits), or neither.
3. **Exposure and gain.** Record the exposure range the SDK reports for each mode, and the gain's effect: the mean of a dark and a bright frame at gain 0, 30 and 240.
4. **Timestamps.** Record what a frame's time field holds, by comparing it with the host's clock over 100 frames.
5. **The line time.** Record the SDK's figure, if it gives one. If not, image a fast-moving edge and measure the shear.
6. **The trigger.** Check whether the SDK's trigger calls succeed on this camera.
7. **The defaults.** Confirm that automatic exposure, automatic white balance and the vertical flip are on at start, and that each can be turned off.

**For the owner to check.**

8. **The camera body:** is there any connector besides USB (trigger, GPIO)?
9. **Lights:** what is fitted (transmitted, reflected), the make and model, and how each is switched and dimmed today: by hand, a controller, USB, or a grblHAL output.
10. **USB:** which port the camera is on.

**The stage controller, at M3's bring-up (H0).** Connect a serial terminal (PuTTY, or `python -m serial.tools.miniterm COM<n> 115200`) with the stage idle and clear. Opening the port can reset some boards.

11. **Build information.** Send `$I`, then `$I+`. Copy both replies into `hardware.md`.
12. **Settings.** Send `$$`. Record:
    - per axis: `$100`–`$102` (steps per mm), `$110`–`$112` (maximum rate), `$120`–`$122` (acceleration) and `$130`–`$132` (travel);
    - `$10`, `$21` and `$22`.
13. **Homing and limits.** Record which switches are fitted, on which axes, and where homing goes.
14. **Encoders.** Record how the linear encoders are read. Send `?` while jogging slowly, and copy a few replies.
15. **The status-report rate.** Run the script below at 10, 50, 100 and 200 Hz. Record the replies per second, and their median and 99th-percentile delay.
16. **Aux outputs.** Record the `[AUX IO:…]` counts, and what is wired to them.

**The host.**

17. **Sustained write.** Run after the 24-hour soak, so that it doesn't disturb it. Write 150 GB sequentially to the NVMe, in 64 MB blocks, flushed. Record the rate for each 10 GB, to see where the drive's cache runs out, and the rate after it. Delete the file afterwards.

**The status-report script** (Python 3 with `pyserial`). It is a tool for this survey, not part of OScope.

```python
"""Polls grblHAL's status report at a set rate, and reports replies per second and delays."""
import statistics, sys, time
import serial

port, rate = sys.argv[1], float(sys.argv[2])  # e.g. COM4 50
with serial.Serial(port, 115200, timeout=0.5) as s:
    s.write(b"\r\n\r\n"); time.sleep(2); s.reset_input_buffer()
    delays, end = [], time.perf_counter() + 10
    while time.perf_counter() < end:
        sent = time.perf_counter()
        s.write(b"?")
        line = s.readline()
        if line.startswith(b"<"):
            delays.append(time.perf_counter() - sent)
        time.sleep(max(0.0, 1 / rate - (time.perf_counter() - sent)))
    delays.sort()
    print(f"{rate} Hz asked: {len(delays) / 10:.1f} replies/s, "
          f"median {statistics.median(delays) * 1e3:.2f} ms, "
          f"p99 {delays[int(0.99 * (len(delays) - 1))] * 1e3:.2f} ms")
```

## Performance

The measurements above are the performance this phase records. None are in yet.

## Hardware

- **The host** was surveyed without opening any port or sending anything to a device.
- **The SDK** was read from its installed documents and headers. Nothing in it was run, and nothing from it is committed.
- **The camera** wasn't attached, and **the stage controller** isn't connected to this host.

## Design notes and deviations from the plan

- **The survey runs in steps,** at the owner's choice:
  - the documentation and the host first;
  - the camera when it is attached;
  - the stage controller at M3's bring-up, because it isn't connected to the host.

  One pull request covers the first two. The controller's items are marked "unknown until H0", which the spec's *Done when* allows.
- **The host is a stand-in.** The owner said the target host is a PC like this one, in CPU, GPU, RAM and SSD, so `hardware.md` records this PC.
- **The SDK's documents were read where they are installed,** and described in our own words. They are Tucsen's, and no SDK file is copied into the repository (ADR-0002).
- **Tucsen's series specification PDF,** linked from older pages, returned 404. The SDK's own MIChrome document took its place.
- **Where no source states a fact,** `hardware.md` says so, rather than guessing.

## Tests

Docs only. The relative links and anchors are checked.

## Follow-ups and known gaps

- **The camera's measurements,** steps 1–7, once it is attached.
- **The owner's checks,** steps 8–10.
- **The stage controller,** steps 11–16, at M3's bring-up (H0).
- **The host's sustained write,** step 17, after the soak.
- **The consequences for the plan** (rolling shutter, timestamps, the trigger, the SDK's defaults, encoders, the scratch disk), applied to the specs in this pull request.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes in this pull request.

## Clean room

Written for this phase from the vendors' documentation: Tucsen's public pages, the TUCam SDK's installed documents and headers, and grblHAL's wiki, all cited in `hardware.md`. Nothing was copied from them, from `microscope`, or from any other codebase.
