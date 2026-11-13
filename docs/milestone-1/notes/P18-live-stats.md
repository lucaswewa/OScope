# P18: Live statistics and overlay data

- Status: Done
- Branch / PR: `m1/p18-live-stats` / [#31](https://github.com/lucaswewa/OScope/pull/31)
- Size: M · estimated ~1,400 before it started (the statistics ~370; their blocks on the preview stream and the sessions ~120; tests ~350; `ex-live-overlays` with its page ~550); one pull request, at the owner's standing choice for P12–P23 · measured **1,955** hand-written lines added (code 749, with 3 in `.gitignore`; tests 592; examples 614), and 47 removed. Docs, `Cargo.lock`, the generated snapshot and P17's stray pending snapshot (429 lines, deleted) aren't counted. **That is 1,355 over M's 600, and about 555 over the estimate:** the statistics module came to 599 lines against ~370, rustfmt spreading its folds over rows across many lines, and the tests to 592 against ~350.

## Summary

Accurate data for the live view's overlays, computed from the raw frames, not the 8-bit preview ([spec](../phases.md#p18), plan §5.5).

- **Live statistics,** in `oscope-acquire`:
  - 12-bit histograms of each channel, from a subsample of about a million samples, with percentiles and the share at full scale;
  - a saturation mask at the preview's resolution, a bit a pixel;
  - raw line profiles, cell by cell;
  - ROI statistics: each channel's mean, standard deviation, minimum and maximum;
  - a focus value: the normalised variance.
- **They go out on the preview WebSocket,** as blocks after the JPEG of the frame they describe, at most 10 times a second, to a client that asks (ADR-0035). The mask goes only while something is saturated.
- **A 20 MP frame's statistics take 11.6 ms** in the live pool, with an ROI, a profile and the mask.
- **`ex-live-overlays`:** the live page with a crosshair, an ROI to draw, a line profile, histograms, false colour through a WebGL lookup table, and the saturation mask.

## What was built

| Where | What |
|---|---|
| `oscope-acquire` `stats.rs` | `measure(frame, request, shown, size)`: `LiveStats` (histograms, percentiles, saturated shares, ROI statistics, profile, focus) and the `Mask`. `StatsRequest` (`roi`, `profile`, `bins`, `mask`) and `Line`. `Mask::to_bytes` and `from_bytes` |
| `oscope-acquire` `preview` | `Preview::measure`: the statistics in the live pool. The wire format's blocks: `Header::message(jpeg, blocks)`, `Header::read` returning a `Message` (header, JPEG, blocks), the JPEG's length at offset 72, the kinds `STATISTICS` and `MASK`, `WireError::Block` |
| `oscope-api` `preview.rs` | The stream's parameter `stats`; each session measures the frame it previews and sends the statistics as JSON and the mask as bits, after the JPEG, its messages with statistics at least 100 ms apart |
| `docs/adr/0035-…` | The statistics' blocks on the preview stream |
| `examples/ex-live-overlays` | Port 8219: the simulator, lit and streaming; the page; `--smoke` |
| `examples/ex-live-view` | Reads the JPEG's length, so its page and smoke check read P18's messages too |

**The measurements,** each on the raw frame, in the sensor's own units (0 to 4095 for 12 bits):

- **Channels:** a mosaic's colours, each sample in its own; grey; or RGB's three.
- **Histograms:** every sample of a subsample of whole cells, a cell every *k* along and down, *k* chosen to keep about 2²⁰ samples, so every colour keeps its share. Counted at 12 bits (fewer levels for fewer bits; 16-bit grey at 4096), then merged into the bins asked for.
- **Percentiles** (0.1, 1, 50, 99 and 99.9 %): the first level whose running count reaches the share.
- **The saturated share:** samples at full scale, of those counted.
- **ROI statistics:** every sample in the ROI, widened to whole cells and clipped to the frame, as the preview's ROI is.
- **The profile:** samples a cell apart along the line, each the cell's channels (a mosaic's two greens averaged), none off the frame.
- **The focus value:** the variance of the green cells over their mean, the normalised variance. It is measured over the ROI, or over the region the preview shows, from a subsample. Groen, Young and Ligthart (1985) compared focus functions, and Sun, Duthaler and Nelson (2005) found the normalised variance among the best for brightfield microscopy.
- **The mask:** a bit for each preview pixel, set where any raw sample of the region it covers is at full scale.

## How to try it

```sh
cargo run -p ex-live-overlays
```

Open `http://127.0.0.1:8219/`. Drag on the image to draw an ROI, shift-drag for a profile, and scroll to zoom. The [README](../../../examples/ex-live-overlays/README.md) says what each overlay shows, how to make the slide saturate (full light, then 25 ms), and how to ask for statistics from your own client.

**What to look for, at review:** the pink saturation mask where the slide is brightest, false colour, the profile's channels, and the focus value falling as you hold z − or z +.

## Performance

**On the owner's PC** (i9-12900, 24 threads, of which the live pool has 6): a 20 MP frame's statistics, with a 1000 × 600 ROI, a profile across the sensor, and the mask for a 1920 × 1280 preview, take **11.6 ms** in the live pool, the median of 9 (`the_cost_at_20_mp`, ignored by default, in release). §5.20 has no budget for them. At 10 Hz that is about an eighth of the live pool's time, and the preview's 34 ms comes on top.

## Design notes and deviations from the plan

**Worth your review:**

- **Statistics travel inside the frame messages, not as messages of their own (ADR-0035).** `teta-wot`'s stream has one slot, so a statistics message sent beside a frame would replace it, or be replaced. As blocks after the JPEG, they arrive with the frame they describe. The header gains the JPEG's length at offset 72; its version stays 1. P17's readers, which take the rest of the message as the JPEG, still work for clients that don't ask.
- **ADR-0014 isn't edited:** accepted ADRs aren't, so the extension is ADR-0035, the next free number, and ADR-0014's status says it is extended.
- **Each session measures for itself,** rather than one statistics subscriber for all clients, as the spec's wording suggests. ROIs, profiles and masks are each client's own. Two clients asking for statistics measure the same frame twice; the frame-wide parts can move to one shared subscriber if that costs too much.
- **The mask is sent only while something is saturated.** It is a bit a pixel, about 300 KB at 1920 × 1280, so a message with statistics and no mask means none.

**Other choices:**

- **Values are in the sensor's units,** not the frame's 16-bit samples: a 12-bit sensor's run to 4095, as microscopists read them.
- **A profile is sampled cell by cell,** so that each sample has every colour. Positions along the line are computed by multiplying before dividing: a line starting on a cell's edge put a sample a hair short of the next edge, and repeated a cell. The end-to-end test found that; a unit test now checks lines of several lengths.
- **Saturation is a sample at full scale:** 4095 for 12 bits. A real camera may clip lower; that will be the camera's to say, in M3.
- **The focus value's scale depends on the light,** as the normalised variance does: it compares positions under the same light and exposure, which is how autofocus (P33) will use it.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-acquire` | `stats` (7, 1 ignored) | Histograms, percentiles and saturated shares against a reference counted in the test, exactly, at 4096 bins and merged to 256; a 2 MP ramp subsampled by whole cells, the colours in proportion, the medians in the middle; ROI statistics against direct sums, widened and clipped, refused when off the frame; profiles cell by cell, off-frame samples empty, on cell edges at several lengths, and on a binned frame; the mask's bits at a quarter size, rows padded, the region shown, none without saturation or when not asked for; the focus value falling with blur, doubling with the light, and 0 on a flat frame; grey 16-bit and RGB 8-bit in their own units; frames not of one plane refused |
| `oscope-acquire` | `the_cost_at_20_mp` (ignored) | The cost in the live pool, in release |
| `oscope-acquire` | `preview`, the wire format | Blocks after the JPEG at ADR-0035's layout, a longer header, P17's 72-byte header read as before, a block cut short refused |
| `oscope-api` | `preview` (4) | `statistics_come_with_the_previews_ten_times_a_second_at_most`: over a real socket, with the fake camera, none unasked. Then each statistics block names its frame, with 64 bins summing to the samples counted, the ROI's counts by colour, a profile whose red follows the fake rows' levels, and a mask exactly on the rows at full scale, at the preview's size. Each is ready at least 100 ms after the last, while the previews switch between a slow one (full size, quality 100, every sample's statistics) and a quick one (tiny) |
| `oscope-api` | `preview`, P17's slow client | Now acknowledges each preview once the camera has published six more frames, rather than after 500 ms: slower than the camera and the session on any machine |
| `oscope-api` | `devices` | The `preview` Thing's description, with `stats`, snapshotted |
| `ex-live-overlays` | `--smoke` | The page; statistics with an ROI and a profile across the sensor; a mask at full light and 25 ms |

**The page** was checked by its smoke test and by a script: `node` read the live stream with the page's own `readMessage`, and found whole JPEGs, statistics at most every 100 ms, and masks at full light. Nobody has looked at it yet; the owner watches it at review.

**Mutation checks** (each broke the code on purpose, then the test named was run):

| Broken on purpose | Caught by |
|---|---|
| Blue samples counted as red | `histograms_percentiles_and_saturation_match_a_reference` |
| Subsampled by pixels, not whole cells | `large_frames_are_subsampled_by_whole_cells` |
| Percentiles a level late | `histograms_percentiles_and_saturation_match_a_reference` |
| Bins folded over, not merged | `histograms_percentiles_and_saturation_match_a_reference` |
| The standard deviation without the mean | `roi_statistics_are_each_channel_s` |
| The mask's bits lowest first | `the_mask_marks_saturated_pixels_at_the_preview_s_resolution` |
| The mask ignores the region shown | `the_mask_marks_saturated_pixels_at_the_preview_s_resolution` |
| A profile ignores binning | `profiles_sample_cell_by_cell` |
| A profile divides before multiplying | `profiles_sample_cell_by_cell` |
| The focus value not normalised | `the_focus_value_falls_as_the_image_blurs`, after adding its check that twice the light doubles it: plain variance falls with blur too |
| Statistics more than 10 times a second | `statistics_come_with_the_previews_ten_times_a_second_at_most` |
| The 100 ms timed from the measuring, not the sending | `statistics_come_with_the_previews_ten_times_a_second_at_most`, **5 of 5 runs on two cores, 3 of 5 on all 24 threads:** it shows only when the slow preview's extra time exceeds the wait for the next frame, which isn't always so on a fast machine. GitHub's runners have two cores |
| P17's slow client: no waiting for acknowledgements; adaptation ignoring frames missed while waiting | `a_slow_client_gets_the_newest_frames`, rerun with its new pacing |
| Statistics nobody asked for | `statistics_come_with_the_previews_ten_times_a_second_at_most` |
| A block's length one long | `the_wire_format_reads_back_what_it_wrote` |
| The JPEG's length ignored | `the_wire_format_reads_back_what_it_wrote` |

**Found by running the API tests on two cores,** as GitHub's runners have, with the test binary pinned to two of this PC's cores before opening the pull request:

- **Statistics came 52 ms apart** in 2 runs of 5. The session timed its 100 ms from deciding to measure, before making the preview. So a preview that took longer to make than the next put two messages with statistics closer than 100 ms, which breaks the promise to the client. Now it times them from the sending, and the test checks exactly 100 ms while making a slow preview and a quick one in turn.
- **P17's slow-client test failed once in 3:** it expected the session to lower its quality, which takes 3 frames missed among 10 messages. With the camera starved to about 3 frames a second, some of the client's 500 ms waits ended before a newer frame came, and rightly weren't misses. The client now waits for six more frames before acknowledging, so it is the slower party by construction.
- Then all four of the API's preview tests passed 5 times out of 5 on two cores.

**A stray file from P17:** P17's merge brought `crates/oscope-api/tests/snapshots/devices__preview.snap.new`, the snapshot insta writes when a snapshot test fails, from its second local replay. P18 deletes it, and `.gitignore` now ignores `*.snap.new`.

## Follow-ups and known gaps

- **One shared statistics subscriber** for the frame-wide parts, if several clients measuring the same frames costs too much.
- **The camera's own saturation level,** for cameras that clip below full scale (M3).
- **Statistics on the GPU:** the CPU's 11.6 ms at 20 MP leaves room for now.
- **Ports:** `ex-live-overlays` takes 8219, the last of the manual-check range (8211–8219). Later examples will have to share a port with one that is never run at the same time, or the range will need widening.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The normalised variance** is from Groen, F. C. A., Young, I. T. and Ligthart, G. (1985), "A comparison of different focus functions for use in autofocus algorithms", *Cytometry* 6(2):81–91. It was ranked by Sun, Y., Duthaler, S. and Nelson, B. J. (2005), "Autofocusing in computer microscopy: selecting the optimal focus algorithm", *Microscopy Research and Technique* 65(3):139–149.
- **Histograms, percentiles, means and standard deviations** are the textbook definitions. The percentile is the first level whose running count reaches the share.
- **The false-colour table** is OScope's own: straight lines between five colours.
