# P17: Preview service and binary WebSocket protocol

- Status: In review
- Branch / PR: `m1/p17-preview` / [#30](https://github.com/lucaswewa/OScope/pull/30)
- Size: L · estimated ~2,900 before it started (the preview chain ~350, a JPEG encoder ~400, the wire format ~150, the `preview` Thing and its sessions ~350, tests ~800, `ex-live-view` ~500); one pull request, at the owner's standing choice for P12–P23 · measured **2,832** hand-written lines added (code 1,394; tests 1,018; examples 420), and 28 removed. Docs, `Cargo.lock` and the generated snapshot aren't counted. **That is 1,932 over L's 900, and about 70 under the estimate.**

## Summary

The light half of the dual stream: each client gets the camera's newest frames as JPEGs the size of its viewport ([spec](../phases.md#p17), plan §5.5, D6).

- **The preview chain,** in `oscope-acquire`: a raw frame's ROI, demosaiced and binned, through the black level, white balance and the display transform, resized and encoded as JPEG. Its first steps run on the GPU when there is one, giving the same bytes as the CPU.
- **OScope's own JPEG encoder,** in `oscope-imaging`, written from ITU-T T.81. `jpeg-encoder`, which the owner chose, carries the Independent JPEG Group's conditions in every version, and ADR-0002 doesn't allow them.
- **The `preview` Thing:** a binary WebSocket at `/api/v1/preview/stream`, the defaults as saved settings, and each session's statistics.
- **The wire format, ADR-0014:** a 72-byte header, then the JPEG.
- **Clients may acknowledge** what they've shown. Then they get at most two messages beyond it, always of the newest frame. Without that, the operating system's socket buffers held seconds of stale frames for a slow client.
- **Adapting:** a client that keeps missing frames gets lower quality, then smaller frames, and gets them back when it keeps up.
- **§5.20's preview budget is met** on this PC: 34.2 ms on the CPU and 22.0 ms with CUDA, against 60 and 30.
- **Jogging moves the image at the stage's speed, the way the mounting says:** measured in a test, 281.6 preview pixels a second against 281.2 expected, towards −x.
- **`ex-live-view`:** a page that shows the stream with WebGL, zooms by ROI, shows fps and latency, and jogs the stage.

## What was built

| Where | What |
|---|---|
| `oscope-imaging` `color/jpeg.rs` | `encode_jpeg`: baseline sequential JPEG (ITU-T T.81) of 8-bit RGB. JFIF's YCbCr, 4:2:0 chroma, Annex K's tables scaled for the quality, an 8 × 8 DCT, Huffman codes built from Annex K's counts. Each row of 16 × 16 units is a restart interval, so rows are encoded in parallel and joined with RST markers |
| `oscope-acquire` `preview/` | `Preview::render`: the chain, in the live pool. `PreviewRequest` (viewport, ROI, quality), `Look` (display transform and colour correction), `Rendered` (the JPEG, the sensor's pixels it shows, its size, CPU or GPU, time) |
| `oscope-acquire` `preview/resize.rs` | The last resize, in 8 bits: a box filter over the fraction that whole-factor binning leaves; grey becomes RGB |
| `oscope-acquire` `preview/adapt.rs` | `Adaptation`: steps of quality, then size, down while a client misses 3 frames in 10, and back up after 50 without a miss |
| `oscope-acquire` `preview/wire.rs` | ADR-0014's header: `Header::message` and `Header::read`, `WireError` |
| `oscope-api` `preview.rs` | The `preview` Thing: settings `defaults`, `look` and `cpu_only`; properties `sessions` and `gpu`; the binary stream `stream`, with its parameters (`width`, `height`, `roi`, `quality`, `max_fps`, `ack`) |
| `oscope-api` `devices` | `Devices` keeps the camera's clock, for the header's times |
| Features | `cuda` on `oscope-acquire`, `oscope-api`, `oscope-server` and `ex-live-view`: previews on the GPU |
| `examples/ex-live-view` | Port 8218. The simulated microscope, homed, over the tissue, lit and streaming; the page; `--smoke` |
| Removed | `jpeg-encoder` from the workspace. P14's JPEG test reads a file from OScope's own encoder |

**The chain,** for a frame and a client's request:

1. **The ROI,** in sensor pixels, mapped to the frame's pixels through the frame's own ROI and binning, clipped to it, and widened to whole 2 × 2 cells of the mosaic. The header says which sensor pixels it shows.
2. **The size:** the ROI fitted inside the viewport, keeping its shape, never larger than the ROI.
3. **Demosaicing:** P08's 2 × 2 superpixel when the output is at most half the ROI each way, so it bins as it goes; P08's bilinear otherwise.
4. **Binning** by the largest whole factor that keeps the image at least the output's size: `downsample`, P06's box.
5. **The frame's black level** subtracted, **white balance** (the `look` setting's gains) for colour frames, and **the display transform** to 8 bits: P08b's steps.
6. **The resize** to the output's size, then **JPEG**.

Steps 3–5 run on the GPU when there is one and `cpu_only` is off, through P08c's CUDA twins and P06's `downsample` twin. Steps 1–2 and 6 always run on the CPU. Everything runs in the live pool.

## How to try it

```sh
cargo run -p ex-live-view
```

Open `http://127.0.0.1:8218/`: the live image, scroll to zoom, the jog buttons with their speed, and the statistics under the image. The [README](../../../examples/ex-live-view/README.md) also shows the stream from your own client, and the Thing's settings with `curl`. `--features cuda` makes the previews on the GPU.

**What to look for, at review:** jogging x + slides the image left, and y + slides it up, at the speed the page says to expect.

## Performance

**On the owner's PC:** i9-12900, 24 threads, of which the live pool has 6 by default; RTX A2000 12 GB. §5.20's budgets are for the owner's laptop (RTX 3070 Ti); this PC stands in for it, at the owner's choice.

**§5.20's preview budget:** a 20 MP frame to a JPEG at 1920 px. `the_preview_budget`, ignored by default, in release; the median of 15, after one to warm up:

| Path | Measured | Budget |
|---|---|---|
| CPU | 34.2 ms, for 1920 × 1280 at quality 95, 286 KB | ≤ 60 ms |
| GPU | 22.0 ms | ≤ 30 ms |

- The frame is a full MIChrome 20 mosaic of 5472 × 3648, with texture and noise, so the JPEG has detail to code.
- The GPU's time includes uploading the 40 MB mosaic, and the resize and JPEG on the CPU.
- Both meet their budgets, the CPU with less margin than the GPU. The laptop has fewer cores than this PC, so its live pool will have fewer threads.

**The JPEG encoder** against zune-jpeg's decoding, on a 640 × 480 picture of gradients, a sharp disc and fine stripes:

| Quality | PSNR | Bytes |
|---|---|---|
| 95 | 36.0 dB | 70,619 |
| 80 | 35.3 dB | 43,175 |
| 50 | 31.2 dB | 23,565 |
| 20 | 27.7 dB | 12,371 |

4:2:0 halves the chroma, so the picture's sharp coloured edges cost a few dB at every quality. A grey gradient at quality 95 is above 40 dB.

**Latency:** in the debug build of `ex-live-view`, from the end of an exposure to the message being ready, about 50 ms once the simulator's tiles were cached, for 684 × 456. That includes acquisition's wait of up to 50 ms for the stage's position.

## Design notes and deviations from the plan

**Worth your review:**

- **OScope encodes its own JPEGs.** `jpeg-encoder`'s DCT, scalar and AVX2, is ported from mozjpeg and libjpeg. So every version carries the Independent JPEG Group's conditions: 0.6.0's MIT or Apache-2.0 metadata was incomplete, and 0.6.1 onwards declares `AND IJG`. IJG isn't among the licences ADR-0002 allows, and adding one is the owner's decision (`deny.toml`).
  - So P17 wrote a baseline encoder from ITU-T T.81, and P14's test reads a JPEG it made.
  - **The spec's "JPEG with SIMD":** it has no hand-written SIMD. It encodes each row of units on its own thread, with restart markers between them, and the compiler vectorises what it can. The budget is met.
  - If the owner allows IJG, `jpeg-encoder` can replace it in one function.
- **The stream is the `preview` Thing's binary stream, `/api/v1/preview/stream`,** not the media plane's `/api/v1/media/preview` (plan §5.4). It gets `teta-wot`'s authentication, shutdown and Thing Description, with the parameters' schema. The plan now says so (§5.4, §5.5 and the change log).
- **Statistics aren't JSON messages from the server:** `teta-wot`'s streams send only binary messages. Each message's header carries its own times, and the Thing's `sessions` property gives each session's last second: fps, latency, bytes a second, sent, missed, quality and size.
- **Clients may acknowledge (ADR-0014), which the spec didn't ask for.** Measured first: a client reading four previews a second through loopback got frames 0 to 7 in order while 27 were published. Windows' socket buffers had taken about 800 KB before the server's sink saw the client was slow. Adapting would have made it worse: smaller messages fit more in the buffers.
  - With acknowledgements, the same client got frames 17, 21 and 25 as 22, 26 and 30 were published.
  - A client that doesn't acknowledge still works, as fast as its socket takes messages.

**Other choices:**

- **The black level is subtracted after demosaicing and binning,** on fewer pixels. Both are means, so it is the same except where noise dips below black.
- **The last resize is in 8 bits, after the display transform.** It covers a factor under 2, left after binning by whole factors, and P08c has no CUDA twin for a fractional resize. Averaging in display values rather than linear light darkens fine bright detail slightly at that step.
- **Each client's preview is made for it alone,** so two clients cost two encodes for each frame.
- **The `preview` Thing makes its own compute layer** when it starts: its pools, and the GPU with `cuda`. P18's statistics will want the same one, so `oscope-api` should then share one with every Thing.
- **`Devices` keeps the camera's clock,** so that the header's ready time is on the same clock as the frame's exposure: the simulator's when the camera is simulated.
- **The simulated camera in the server still renders on the CPU.** P15 put it on the GPU only in its example. The preview's own steps use the GPU with `cuda`.
- **An ROI that misses the frame shows the whole frame,** rather than nothing. The header says what was sent.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-imaging` | `jpeg` (5) | zune-jpeg decodes each quality close to the picture, with sizes falling as quality falls; awkward sizes (1 × 1, 37 × 23, 16 × 17, 300 × 9); a flat colour within 2; a grey gradient above 40 dB; restart markers in order; a wrong buffer refused |
| `oscope-imaging` | `read` | P14's JPEG test, from OScope's encoder. Its blocks share one chroma, since 4:2:0 halves the chroma's resolution |
| `oscope-acquire` | `preview` (7, 1 ignored, 1 CUDA) | ROIs mapped from the sensor to the frame and back: offset, odd, clipped, missing, empty, binned, with a square landing where it should; the viewport fitted, never enlarged; colours, the black level and white balance through the superpixel and bilinear paths, and grey frames; frames that aren't one plane refused; adaptation down and back up; the wire format written and read at ADR-0014's offsets, a longer header from a later writer, and what isn't a message refused |
| `oscope-acquire` | `the_gpu_s_previews_are_the_cpu_s` | With `OSCOPE_CUDA=1`: the GPU's JPEGs are the CPU's, byte for byte, binned, unbinned, zoomed, white-balanced, over a black level |
| `oscope-acquire` | `the_preview_budget` (ignored) | §5.20, in release, CPU and GPU |
| `oscope-acquire` | `resize` (2, unit) | the box's weights cover each output once; a ramp keeps its means, and grey becomes RGB |
| `oscope-api` | `preview` (3) | Over a real socket: a client's ROI, size and quality; a new message replacing the last; an ROI that misses showing the whole frame; `max_fps`; unreadable parameters answered with an error; the session's statistics, and its end with its socket. A slow client that acknowledges gets the newest frames, missing those between, and its quality comes down. Jogging the simulator at 540 µm/s moves the image at 281.6 px/s against 281.2 expected, towards −x, with no sideways shift |
| `oscope-api` | `devices` | The `preview` Thing's description snapshotted, and its routes in the OpenAPI document |
| `ex-live-view` | `--smoke` | The page; four previews during a jog, acknowledged; one session |

**The jog's measurement:** ten previews of the half-size frame during a jog at 540 µm/s, decoded to grey. Each pair's shift is where they differ least, within ±80 pixels along x and ±6 along y, refined to a fraction along x. Each pair's time comes from their headers. The rate is the sum of the shifts over the sum of the times. The expected rate is the speed over the objective's pixel at the sample (`pixel_at_sample`, 0.96 µm), times the preview's scale. The direction comes from the simulator's mounting, neither turned nor mirrored: the stage moving +x shows the slide further along +x, so the image moves −x.

**Mutation checks** (each broke the code on purpose, then the test named was run):

| Broken on purpose | Caught by |
|---|---|
| Adaptation ignores drops | `a_client_that_keeps_dropping_gets_less_then_more` |
| The session doesn't wait for acknowledgements | `a_slow_client_gets_the_newest_frames` |
| The session ignores its adaptation's quality | `a_slow_client_gets_the_newest_frames` |
| Adaptation ignores the frames a client missed while the session waited for it | `a_slow_client_gets_the_newest_frames` |
| Sessions are never removed | `a_client_s_parameters_shape_its_previews` |
| No fallback for an ROI that misses | `a_client_s_parameters_shape_its_previews` |
| `max_fps` ignored | `a_client_s_parameters_shape_its_previews` |
| ROIs ignore binning | `rois_map_from_the_sensor_to_the_frame_and_back` |
| The black level skipped | `colours_the_black_level_and_white_balance_come_through` |
| The resize's weights not normalised | `the_weights_cover_each_output_once` |
| The wire format reads x for y | `the_wire_format_reads_back_what_it_wrote` |
| No 0 after a 0xff in the JPEG's data | the JPEG tests |
| Chroma quantised with the luminance table | the JPEG tests |
| Restart markers misnumbered | **missed at first:** zune-jpeg resyncs on any restart marker, but libjpeg, and so browsers, check the numbers. `restart_markers_follow_each_row_in_order` now reads them from the file, and catches it |

The `--smoke` checks of every example, and the slow-client and jog tests, wait for conditions, with deadlines, never for fixed times. The slow client's 250 ms between reads is the slowness under test.

## Follow-ups and known gaps

- **The owner's decision on IJG:** keep OScope's encoder, or allow IJG and use `jpeg-encoder`.
- **Upstream `teta-wot`:** text messages from the server on a binary stream, for statistics and dirty mosaic tiles (P30). Worked around here.
- **One compute layer for the server,** shared by the preview, P18's statistics and later jobs.
- **The simulated camera on the GPU in the server,** as P15's example does.
- **The page has been checked by its smoke test and a script, not by eye:** `node` read the live stream with the page's own header parser, acknowledging as it does. The owner watches it at review.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The JPEG encoder** follows ITU-T T.81 (ISO/IEC 10918-1):
  - its DCT, A.3.3;
  - its quantisation and zig-zag order, A.3.4 and A.3.6;
  - its example tables, Annex K.1 and K.3, scaled for the quality as the IJG's documented convention does (5000 / q below 50, 200 − 2q above), a published formula;
  - building Huffman codes from their counts, Annex C;
  - DC differences and AC run lengths, F.1.2;
  - restart intervals, B.2.4.4.

  The colour conversion and APP0 are JFIF's (ITU-T T.871). `jpeg-encoder`'s source was read only to check its licence.
- **The resize's box filter** weighs each input pixel by the area of the output pixel it covers: the standard area average.
- **The preview's other steps** are P06's, P08's and P08b's.
