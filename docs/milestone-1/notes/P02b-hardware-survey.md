# P02b: Hardware capability survey (docs only)

- Status: In review
- Branch / PR: `m1/p02b-hardware-survey` / [#10](https://github.com/lucaswewa/OScope/pull/10)
- Size: S, docs only, so it isn't counted

## Summary

[`docs/hardware.md`](../../hardware.md) records what the real camera, SDK, stage controller, lights and host offer ([spec](../phases.md#p02b)). Its facts come from:

- the vendors' documentation;
- the TUCam SDK, installed on the host;
- the owner's earlier MIChrome 20 test tool, with its measurements;
- the host, a PC like the target one;
- the owner's answers.

**Nothing in hardware was connected.** The camera isn't connected to the host yet. The stage controller isn't either, so at the owner's word OScope runs on the simulated stage until M3, and the controller's items wait for M3's bring-up (H0). The lights aren't chosen, and are simulated until they are.

**What the survey found, and how the plan changes:**

- **The MIChrome 20 has a rolling shutter:** about 60 ms of readout at full resolution. The frame metadata gains a line time (§5.5, P03), and the simulated camera exposes its rows in turn (P12).
- **It gives no timestamps,** and the SDK's frame index stays 0. Frames are timed on the host, and a sensor timestamp becomes optional.
- **It has no trigger: USB only.** A camera with an external trigger is to replace it. So the HAL keeps external triggering as a capability (§5.9, P09), and the simulator's trigger stands in for the new camera.
- **Its raw RGGB mosaic is processed by the SDK:**
  - gamma, contrast and white balance are applied;
  - it is rescaled to 16 bits with a black offset of about 2,000;
  - it is linear at gamma 128 and contrast 32.

  M3's driver sets that up, and P12 simulates the same output.
- **The measured frame rates:** 14.5, 53.2 and 67.3 fps in its three modes.
- **The lights aren't chosen,** so `Illumination` stays generic (§5.9, P09).

## What was built

- **`docs/hardware.md`:** the camera, SDK, stage controller, lights, host and environment; the consequences; and the sources.
- **The plan:**
  - §1's instrument;
  - D20;
  - §5.5's frame metadata;
  - §5.9's camera capabilities and illumination;
  - M3's H2 and H5;
  - the camera risk;
  - a change-log line;
  - P01 and P02 already *Done*, and P02b *In review*.
- **The phase specs:** P03 (frame metadata), P09 (triggering as a capability, generic lights) and P12 (rolling shutter, processed raw output, measured modes, host timestamps, shear test).
- **The README's instrument table,** and the docs index's row for `hardware.md`.

## How to try it

How to take, or repeat, each measurement that is still open.

**The camera, once it is connected.** No OScope code is needed: the owner's test tool, or the SDK's samples, will do.

1. **The raw mosaic's frame rate at full resolution,** 16-bit, with a 1 ms exposure. Record whether it reaches 14.5 fps, about 580 MB/s. The 16-bit colour output gives only 7.3 fps.
2. **ROI:** whether the camera accepts one, and its frame rate.
3. **USB:** which port and controller the camera is on.

**The stage controller, at M3's bring-up (H0).** Connect a serial terminal (PuTTY, or `python -m serial.tools.miniterm COM<n> 115200`) with the stage idle and clear. Opening the port can reset some boards.

4. **Build information.** Send `$I`, then `$I+`. Copy both replies into `hardware.md`.
5. **Settings.** Send `$$`. Record:
   - per axis: `$100`–`$102` (steps per mm), `$110`–`$112` (maximum rate), `$120`–`$122` (acceleration) and `$130`–`$132` (travel);
   - `$10`, `$21` and `$22`.
6. **Homing and limits.** Record which switches are fitted, on which axes, and where homing goes.
7. **Encoders.** Record how the linear encoders are read. Send `?` while jogging slowly, and copy a few replies.
8. **The status-report rate.** Run the script below at 10, 50, 100 and 200 Hz. Record the replies per second, and their median and 99th-percentile delay.
9. **Aux outputs.** Record the `[AUX IO:…]` counts, and what is wired to them.

**The host.**

10. **Sustained write.** Write 150 GB sequentially to the NVMe in 64 MB blocks of random data, flushed to the disk every 1 GB. Record the rate for each 10 GB, to see where the drive's cache runs out, and the rate after it. Delete the file afterwards. It runs after U06's 24-hour soak, so that it doesn't disturb it.

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

The camera's frame rates come from the owner's test tool (`hardware.md`). The disk's sustained write rate is still to come: DISK_RESULT

## Hardware

- **The host** was surveyed without opening any port or sending anything to a device.
- **The SDK** was read from its installed documents and headers. Nothing in it was run, and nothing from it is committed.
- **The owner's test tool** was read for its documented measurements and findings. Its code wasn't used, and the tool and its images aren't committed: the folder is excluded in this clone's `.git/info/exclude`.
- **The camera and the stage controller** weren't connected.

## Design notes and deviations from the plan

- **The survey uses what exists today,** at the owner's choice:
  - the documentation, the installed SDK and the test tool's measurements now;
  - the camera's last few items once it is connected;
  - the stage controller at M3's bring-up, as it isn't connected to the host.

  The spec's *Done when* allows items marked unknown with a reason. Here the reasons are "when connected" and "unknown until H0".
- **The host is a stand-in.** The target host is a PC like this one (owner).
- **The camera's successor shapes the HAL,** at the owner's word: external triggering stays a camera capability, though the MIChrome 20 has none. P11 and P12 keep their trigger output and input, standing in for the new camera.
- **The lights stay generic and simulated,** at the owner's word, until they are chosen.
- **The specs were changed only where a finding contradicted them,** as the spec asks.
  - **§5.20's budgets aren't changed.** The raw mosaic's rate at full resolution is unmeasured, so the 580 MB/s recording budget stands until it is.
  - **P17 isn't changed.** Nothing found affects the preview.
- **The 49 Hz shake of the test table** is recorded in `hardware.md`, but changes no spec. The final table may differ. Autofocus, live stitching and long exposures will show any shake that remains.
- **The test tool and the SDK's documents are described in our own words.** They are the owner's and Tucsen's, and no file of either is copied into the repository (ADR-0002).
- **Tucsen's older specification PDF** returned 404. The SDK's own MIChrome document took its place.

## Tests

Docs only. The relative links and anchors are checked.

## Follow-ups and known gaps

- **When the camera is connected:** the raw mosaic's frame rate at full resolution, ROI, and the USB port (steps 1–3).
- **At H0:** the stage controller (steps 4–9).
- **The host's sustained write** (step 10), after U06's soak. Its result goes into `hardware.md` and these notes, in this pull request.
- **A camera with an external trigger,** to be chosen; and the lights.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes in this pull request.

## Clean room

Written for this phase from the sources cited in `hardware.md`:

- Tucsen's public pages, and the TUCam SDK's installed documents and headers;
- grblHAL's wiki;
- the owner's test tool's README.

Nothing was copied from them, from `microscope`, or from any other codebase.
