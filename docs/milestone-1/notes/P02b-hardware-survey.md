# P02b: Hardware capability survey (docs only)

- Status: In progress: the documentation and the host are surveyed; the instrument's measurements wait for the owner
- Branch / PR: `m1/p02b-hardware-survey` / not opened yet
- Size: S, docs only, so it isn't counted

## Summary

[`docs/hardware.md`](../../hardware.md) records what the real camera, SDK, stage controller, lights and host offer ([spec](../phases.md#p02b)). This draft holds what the vendors publish, and the host PC, which the owner confirmed will run the instrument. The measurements below wait for the owner at the instrument. The pull request opens once they are in.

Two findings may change the plan already:

- **the camera has a rolling shutter,** which the plan doesn't mention;
- **its published specifications list no hardware trigger,** which the plan assumes.

## What was built

- `docs/hardware.md`: camera, SDK, stage controller, lights, host, the consequences for the plan, and the sources.
- `docs/README.md`: its row links the new page.

## How to try it

**The checklist for the session at the instrument.** Each step says what to record in `hardware.md`. Nothing here needs OScope's code. The camera steps use the vendor's own software and SDK samples; the controller steps use a serial terminal and the script below.

**Camera and SDK**

1. **Install the SDK and the driver.** Use the "Tucsen SDK Kit for Windows" (`TUCam_FV2.0.8.0_PV2.0.151.0`) and the TUCam camera driver from the MIChrome 20 page. Record:
   - the versions installed;
   - the kit's licence file, and whether it lets OScope ship the SDK's libraries.
2. **Modes and frame rates.** In Mosaic, or the SDK's sample, with a short exposure (1 ms), stream for 10 s at each of:
   - full resolution;
   - 2 × 2, 3 × 3 and 4 × 4 binning;
   - a small ROI, if one can be set.

   Record the frame rate each shows, and whether an ROI can be set.
3. **Raw data.** In the SDK's documentation or sample, find whether a raw (not demosaiced) frame can be read, and its pixel format. Save one raw 16-bit frame of a bright, even field. Record:
   - the Bayer pattern;
   - whether the values are all multiples of 16 (12 significant bits, aligned high), of 4 (14 bits), or neither.
4. **Exposure and gain.** Record the ranges, and the gain modes, the SDK reports.
5. **Timestamps.** Record whether a frame carries a timestamp from the camera or from the host, and its resolution.
6. **The line time.** Record the SDK's figure, if it reports one. If not, image a fast-moving edge and measure the shear.
7. **The trigger.** Look for a trigger connector on the camera body, and for trigger modes in the SDK. Record what exists: input, output, edge or level, and the delay if stated.
8. **Other controls.** Record white balance, temperature readout, and anything else the SDK offers.

**Stage controller (grblHAL)**

Connect a serial terminal (PuTTY, or `python -m serial.tools.miniterm COM<n> 115200`) with the stage idle and clear. Opening the port can reset some boards.

9. **Build information.** Send `$I`, then `$I+`. Copy both replies into `hardware.md`.
10. **Settings.** Send `$$`. Record:
    - per axis: `$100`–`$102` (steps per mm), `$110`–`$112` (maximum rate), `$120`–`$122` (acceleration) and `$130`–`$132` (travel);
    - `$10` (the report mask), `$21` (hard limits) and `$22` (homing).
11. **Homing and limits.** Record which switches are fitted, on which axes, and where homing goes.
12. **Encoders.** Record how the linear encoders are read today: through grblHAL (a plugin, and in which report?), or a separate readout. Send `?` while jogging slowly, and copy a few replies.
13. **The status-report rate.** Run the script below for each rate. Record, at each requested rate, the replies per second and their median and 99th-percentile delay.
14. **Aux outputs and the trigger wiring.** Record the `[AUX IO:…]` counts, and which output, if any, is wired to the camera. With the camera unplugged, check `M64 P0` / `M65 P0` switch it.

**Lights**

15. Record what is fitted (transmitted, reflected), its make and model, and how each is switched and dimmed today: by hand, a controller, USB, or a grblHAL output.

**Host**

16. **USB.** Record which port and controller the camera is on (USB 3.2 or 3.1).
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

Run it at 10, 50, 100 and 200 Hz.

## Performance

The measurements above are the performance this phase records. None are in yet.

## Hardware

The host was surveyed without opening any port or sending anything to a device. The camera and the controller weren't connected.

## Design notes and deviations from the plan

- **The survey was split in two,** at the owner's choice: the documentation and the host first, the instrument when the owner is there. One pull request covers both.
- **The SDK wasn't downloaded or installed yet.** Its licence is unknown, and its documentation ships inside the kit; it is the first step of the session.
- **Tucsen's series specification PDF** linked from older pages returned 404. The product page lists a newer one (2026-05-26), still to read.
- **The facts come from the vendors' public pages and grblHAL's wiki,** as listed in `hardware.md`. Where a page doesn't state something, `hardware.md` says so rather than guessing.

## Tests

Docs only. The relative links and anchors are checked.

## Follow-ups and known gaps

- **Everything marked to measure or to check** in `hardware.md`.
- **The consequences for the plan** (rolling shutter, the trigger, encoders, the scratch disk), applied to the specs once the measurements are in.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes in this pull request.

## Clean room

Written for this phase from the vendors' public documentation, cited in `hardware.md`. Nothing was copied from `microscope` or any other codebase.
