//! The preview chain: ROIs mapped from the sensor to the frame and back,
//! the viewport, colours and the black level, adaptation, and the wire
//! format; with `OSCOPE_CUDA=1`, the GPU's previews against the CPU's; and,
//! ignored, §5.20's budget.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::preview::wire::{FRAME, Header, JPEG, MAGIC, MASK, STATISTICS, WireError};
use oscope_acquire::preview::{Adaptation, Look, Preview, PreviewError, PreviewRequest, Rendered};
use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{
    BayerPattern, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, PixelFormat, PixelRect,
    Timestamp,
};
use oscope_imaging::color::ColourCorrection;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

const RGGB: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};

fn preview(config: &ComputeConfig) -> Preview {
    Preview::new(Arc::new(Compute::new(config).unwrap()))
}

/// A small preview: two live threads.
fn small() -> Preview {
    preview(&ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(1),
        cuda_device: None,
    })
}

/// A frame of `format`, `width` × `height`, of the sensor's pixels at
/// `roi` binned by `binning`: each sample `value(x, y)`, from 0 to 1.
fn frame(
    format: PixelFormat,
    (width, height): (u32, u32),
    roi: PixelRect,
    binning: u32,
    value: impl Fn(u32, u32) -> f64,
) -> Frame {
    let pool = FramePool::new(format.frame_bytes(width, height), 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    let (bytes, channels) = (format.sample_bytes(), format.channels() as u32);
    for (i, sample) in buffer.bytes_mut().chunks_exact_mut(bytes).enumerate() {
        let pixel = i as u32 / channels;
        let v = value(pixel % width, pixel / width).clamp(0.0, 1.0);
        match bytes {
            2 => sample.copy_from_slice(&((v * 65_535.0).round() as u16).to_le_bytes()),
            _ => sample[0] = (v * 255.0).round() as u8,
        }
    }
    let mut meta = FrameMeta::new(FrameId(7), format, width, height, Timestamp(0));
    meta.roi = roi;
    meta.binning = binning;
    Frame::new(buffer, meta).unwrap()
}

/// The value at `(x, y)` of an RGGB mosaic of one colour.
fn rggb([r, g, b]: [f64; 3]) -> impl Fn(u32, u32) -> f64 {
    move |x, y| match (x % 2, y % 2) {
        (0, 0) => r,
        (1, 1) => b,
        _ => g,
    }
}

fn request(viewport: (u32, u32), roi: Option<PixelRect>) -> PreviewRequest {
    PreviewRequest {
        viewport,
        roi,
        quality: 95,
    }
}

/// The JPEG's RGB.
fn decode(rendered: &Rendered) -> Vec<u8> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(&rendered.jpeg), options);
    let pixels = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    assert_eq!(
        (u32::from(info.width), u32::from(info.height)),
        (rendered.width, rendered.height)
    );
    pixels
}

/// The mean of the decoded RGB over the columns `xs` and rows `ys`.
fn mean(rgb: &[u8], width: u32, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> f64 {
    let (count, mut sum) = (xs.len() * ys.len() * 3, 0.0);
    for y in ys {
        for x in xs.clone() {
            let at = 3 * (y * width + x) as usize;
            sum += rgb[at..at + 3].iter().map(|&v| f64::from(v)).sum::<f64>();
        }
    }
    sum / count as f64
}

#[test]
fn rois_map_from_the_sensor_to_the_frame_and_back() {
    // A frame of the sensor's pixels from (120, 60), dark but for a white
    // square from (400, 300) to (440, 340) on the sensor.
    let at = PixelRect::new(120, 60, 480, 360);
    let square = |x: u32, y: u32| {
        let (sx, sy) = (x + 120, y + 60);
        match (400..440).contains(&sx) && (300..340).contains(&sy) {
            true => 1.0,
            false => 0.02,
        }
    };
    let frame = frame(RGGB, (480, 360), at, 1, square);
    let preview = small();
    let look = Look::default();
    let shown = |roi| preview.render(&frame, &request((1000, 1000), Some(roi)), &look, false);

    let rendered = shown(PixelRect::new(380, 280, 100, 100)).unwrap();
    assert_eq!(rendered.roi, PixelRect::new(380, 280, 100, 100));
    assert_eq!((rendered.width, rendered.height), (100, 100));
    let rgb = decode(&rendered);
    assert!(mean(&rgb, 100, 22..58, 22..58) > 240.0);
    assert!(mean(&rgb, 100, 0..100, 0..15) < 60.0);
    assert!(mean(&rgb, 100, 65..100, 0..100) < 60.0);

    // Widened to whole cells; clipped to the frame; refused when it misses.
    let odd = shown(PixelRect::new(381, 281, 99, 98)).unwrap();
    assert_eq!(odd.roi, PixelRect::new(380, 280, 100, 100));
    let clipped = shown(PixelRect::new(500, 300, 400, 400)).unwrap();
    assert_eq!(clipped.roi, PixelRect::new(500, 300, 100, 120));
    assert!(matches!(
        shown(PixelRect::new(0, 0, 100, 50)),
        Err(PreviewError::Roi { .. })
    ));
    assert!(matches!(
        shown(PixelRect::new(400, 300, 0, 10)),
        Err(PreviewError::Roi { .. })
    ));

    // Binned 2 × 2: each frame pixel is two sensor pixels each way.
    let binned = self::frame(
        RGGB,
        (480, 360),
        PixelRect::new(0, 0, 960, 720),
        2,
        |_, _| 0.5,
    );
    let rendered = preview
        .render(
            &binned,
            &request((1000, 1000), Some(PixelRect::new(401, 300, 200, 200))),
            &look,
            false,
        )
        .unwrap();
    assert_eq!(rendered.roi, PixelRect::new(400, 300, 204, 200));
    assert_eq!((rendered.width, rendered.height), (102, 100));
}

#[test]
fn the_output_fits_the_viewport_and_is_never_enlarged() {
    let frame = frame(
        RGGB,
        (480, 360),
        PixelRect::new(0, 0, 480, 360),
        1,
        |_, _| 0.3,
    );
    let preview = small();
    for (viewport, size) in [
        ((240, 240), (240, 180)),
        ((2000, 100), (133, 100)),
        ((4000, 4000), (480, 360)),
        ((0, 0), (1, 1)),
        ((97, 1000), (97, 73)),
    ] {
        let rendered = preview
            .render(&frame, &request(viewport, None), &Look::default(), false)
            .unwrap();
        assert_eq!((rendered.width, rendered.height), size, "{viewport:?}");
        decode(&rendered);
    }
}

#[test]
fn colours_the_black_level_and_white_balance_come_through() {
    // Linear 0.5, 0.25 and 0.125 over a black level of a tenth: sRGB's 188,
    // 137 and 99, binned by the superpixel, then 3 × 3.
    let colour = rggb([0.5, 0.25, 0.125]);
    let mut frame = frame(
        RGGB,
        (720, 540),
        PixelRect::new(0, 0, 720, 540),
        1,
        |x, y| 0.1 + 0.9 * colour(x, y),
    );
    let mut meta = frame.meta().clone();
    meta.black_level = 0.1 * 65_535.0;
    frame = Frame::new(frame_buffer(&frame), meta).unwrap();
    let preview = small();
    let centre = |rendered: &Rendered| {
        let rgb = decode(rendered);
        let at = 3 * (rendered.height / 2 * rendered.width + rendered.width / 2) as usize;
        [rgb[at], rgb[at + 1], rgb[at + 2]]
    };
    let look = Look::default();
    let rendered = preview
        .render(&frame, &request((120, 120), None), &look, false)
        .unwrap();
    assert_eq!((rendered.width, rendered.height), (120, 90));
    let close = |got: [u8; 3], want: [u8; 3]| got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2);
    assert!(
        close(centre(&rendered), [188, 137, 99]),
        "{:?}",
        centre(&rendered)
    );

    // White balance makes it grey; bilinear, unbinned, keeps it.
    let balanced = Look {
        correction: ColourCorrection {
            gains: [1.0, 2.0, 4.0],
            ..ColourCorrection::default()
        },
        ..look
    };
    for viewport in [(120, 120), (1000, 1000)] {
        let rendered = preview
            .render(&frame, &request(viewport, None), &balanced, false)
            .unwrap();
        assert!(
            close(centre(&rendered), [188; 3]),
            "{:?}",
            centre(&rendered)
        );
    }

    // Grey frames come out grey.
    let grey = self::frame(
        PixelFormat::Mono8,
        (64, 48),
        PixelRect::new(0, 0, 64, 48),
        1,
        |_, _| 0.5,
    );
    let rendered = preview
        .render(&grey, &request((32, 32), None), &look, false)
        .unwrap();
    assert!(
        close(centre(&rendered), [188; 3]),
        "{:?}",
        centre(&rendered)
    );
}

/// A copy of `frame`'s pixels, in a buffer of its own.
fn frame_buffer(frame: &Frame) -> oscope_core::FrameBuffer {
    let pool = FramePool::new(frame.bytes().len(), 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    buffer.bytes_mut().copy_from_slice(frame.bytes());
    buffer
}

#[test]
fn frames_that_arent_one_plane_of_samples_are_refused() {
    let format = PixelFormat::PlanarF32 { planes: 2 };
    let pool = FramePool::new(format.frame_bytes(8, 8), 1, &HeapAllocator);
    let meta = FrameMeta::new(FrameId(1), format, 8, 8, Timestamp(0));
    let frame = Frame::new(pool.take().unwrap(), meta).unwrap();
    let refused = small().render(&frame, &request((8, 8), None), &Look::default(), false);
    assert_eq!(refused.unwrap_err(), PreviewError::Format(format));
}

#[test]
fn a_client_that_keeps_dropping_gets_less_then_more() {
    let mut adaptation = Adaptation::default();
    assert_eq!((adaptation.quality(80), adaptation.scale()), (80, 1.0));
    // One drop in ten changes nothing.
    for i in 0..30 {
        adaptation.record(i % 10 == 0);
    }
    assert_eq!(adaptation.step(), 0);
    // Three drops take a step down: quality first, to 30.
    let down = |adaptation: &mut Adaptation| (0..3).for_each(|_| adaptation.record(true));
    down(&mut adaptation);
    assert_eq!((adaptation.quality(80), adaptation.scale()), (65, 1.0));
    (0..3).for_each(|_| down(&mut adaptation));
    assert_eq!((adaptation.quality(80), adaptation.scale()), (30, 1.0));
    assert_eq!(
        adaptation.quality(20),
        20,
        "a client asking for less gets it"
    );
    // Then the size, to a quarter.
    down(&mut adaptation);
    assert_eq!(adaptation.scale(), 0.75);
    (0..10).for_each(|_| down(&mut adaptation));
    assert_eq!((adaptation.quality(80), adaptation.scale()), (30, 0.25));
    // Fifty messages without a drop take a step back up: size first.
    let deepest = adaptation.step();
    (0..49).for_each(|_| adaptation.record(false));
    assert_eq!(adaptation.step(), deepest);
    adaptation.record(false);
    assert_eq!(adaptation.step(), deepest - 1);
    (0..50 * deepest).for_each(|_| adaptation.record(false));
    assert_eq!((adaptation.quality(80), adaptation.scale()), (80, 1.0));
}

fn header() -> Header {
    Header {
        kind: FRAME,
        codec: JPEG,
        quality: 80,
        sequence: 0x0102_0304,
        frame: FrameId(u64::MAX - 5),
        exposure_start: Timestamp(1_000_000_007),
        exposure_end: Timestamp(1_010_000_009),
        ready: Timestamp(1_030_000_011),
        roi: PixelRect::new(12, 24, 5448, 3600),
        width: 1920,
        height: 1269,
    }
}

#[test]
fn the_wire_format_reads_back_what_it_wrote() {
    let message = header().message(b"jpeg bytes", &[]);
    assert_eq!(message.len(), Header::LEN + 10);
    assert_eq!(message[..4], MAGIC);
    // Little-endian, at the offsets ADR-0014 and ADR-0035 give.
    assert_eq!(message[8..12], 76_u32.to_le_bytes());
    assert_eq!(message[12..16], [4, 3, 2, 1]);
    assert_eq!(message[64..68], 1920_u32.to_le_bytes());
    assert_eq!(message[72..76], 10_u32.to_le_bytes());
    let read = Header::read(&message).unwrap();
    assert_eq!((read.header, read.jpeg), (header(), &b"jpeg bytes"[..]));
    assert!(read.blocks.is_empty());

    // Blocks after the JPEG: each its kind, three zeros, its length, then
    // its bytes.
    let statistics = br#"{"focus": 1}"#;
    let blocks = header().message(b"jpeg", &[(STATISTICS, statistics), (MASK, &[1, 2, 3])]);
    let at = Header::LEN + 4;
    assert_eq!(blocks[at..at + 8], [STATISTICS, 0, 0, 0, 12, 0, 0, 0]);
    let read = Header::read(&blocks).unwrap();
    assert_eq!(read.jpeg, b"jpeg");
    assert_eq!(
        read.blocks,
        vec![(STATISTICS, &statistics[..]), (MASK, &[1, 2, 3][..])]
    );

    // A longer header, from a later writer: its payload starts where it says.
    let mut longer = message[..Header::LEN].to_vec();
    longer[8..12].copy_from_slice(&84_u32.to_le_bytes());
    longer[72..76].copy_from_slice(&4_u32.to_le_bytes());
    longer.extend_from_slice(&[0xee; 8]);
    longer.extend_from_slice(b"jpeg");
    assert_eq!(Header::read(&longer).unwrap().jpeg, b"jpeg");

    // P17's 72-byte header has no JPEG length: its JPEG is the rest.
    let mut older = message[..72].to_vec();
    older[8..12].copy_from_slice(&72_u32.to_le_bytes());
    older.extend_from_slice(b"jpeg");
    let read = Header::read(&older).unwrap();
    assert_eq!((read.header, read.jpeg), (header(), &b"jpeg"[..]));
}

#[test]
fn the_wire_format_refuses_what_isnt_a_message() {
    let message = header().message(b"", &[]);
    assert_eq!(Header::read(&message[..71]), Err(WireError::Short(71)));
    let mut wrong = message.clone();
    wrong[0] = b'X';
    assert_eq!(Header::read(&wrong), Err(WireError::Magic));
    let mut newer = message.clone();
    newer[4] = 2;
    assert_eq!(Header::read(&newer), Err(WireError::Version(2)));
    let mut past_the_end = message;
    past_the_end[8..12].copy_from_slice(&200_u32.to_le_bytes());
    assert_eq!(Header::read(&past_the_end), Err(WireError::Short(76)));
    let blocked = header().message(b"j", &[(MASK, &[0; 16])]);
    let cut = &blocked[..blocked.len() - 1];
    assert_eq!(Header::read(cut), Err(WireError::Block));
}

/// A 20 MP frame of the MIChrome 20: a textured mosaic.
fn full_frame() -> Frame {
    frame(
        RGGB,
        (5472, 3648),
        PixelRect::new(0, 0, 5472, 3648),
        1,
        |x, y| {
            let hash = (u64::from(x) << 32 | u64::from(y)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let noise = (hash >> 40) as f64 / f64::from(1 << 24);
            0.3 + 0.2 * (f64::from(x) / 300.0).sin() * (f64::from(y) / 200.0).cos() + 0.05 * noise
        },
    )
}

/// The median of `runs` previews of `frame` at 1920 px, after one to warm
/// up.
fn median(preview: &Preview, frame: &Frame, gpu: bool, runs: usize) -> (Duration, Rendered) {
    let request = request((1920, 1920), None);
    let look = Look::default();
    let mut rendered = preview.render(frame, &request, &look, gpu).unwrap();
    let mut took: Vec<Duration> = (0..runs)
        .map(|_| {
            rendered = preview.render(frame, &request, &look, gpu).unwrap();
            rendered.took
        })
        .collect();
    took.sort();
    (took[runs / 2], rendered)
}

/// §5.20: a 20 MP frame to a JPEG at 1920 px in 60 ms on the CPU, and 30
/// with CUDA. Run it with `cargo test --release -p oscope-acquire --test
/// preview -- --ignored --nocapture`, and `--features cuda` for the GPU.
#[test]
#[ignore = "a benchmark: run it in release"]
fn the_preview_budget() {
    let frame = full_frame();
    let preview = preview(&ComputeConfig::default());
    let (cpu, rendered) = median(&preview, &frame, false, 15);
    println!(
        "CPU: {:.1} ms for {} × {}, {} bytes",
        cpu.as_secs_f64() * 1e3,
        rendered.width,
        rendered.height,
        rendered.jpeg.len()
    );
    // What `cargo xtask perf` reads (P53).
    println!("perf: preview.cpu_ms {:.2}", cpu.as_secs_f64() * 1e3);
    assert_eq!((rendered.width, rendered.height), (1920, 1280));
    assert!(cpu <= Duration::from_millis(60), "{cpu:?}");
    if preview.has_gpu() {
        let (gpu, rendered) = median(&preview, &frame, true, 15);
        assert!(rendered.gpu);
        println!("GPU: {:.1} ms", gpu.as_secs_f64() * 1e3);
        println!("perf: preview.gpu_ms {:.2}", gpu.as_secs_f64() * 1e3);
        assert!(gpu <= Duration::from_millis(30), "{gpu:?}");
    }
}

/// With `OSCOPE_CUDA=1`, the GPU's previews are the CPU's, to the byte.
#[cfg(feature = "cuda")]
#[test]
fn the_gpu_s_previews_are_the_cpu_s() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return;
    }
    let preview = preview(&ComputeConfig::default());
    assert!(preview.has_gpu(), "OSCOPE_CUDA=1, but no GPU");
    let colour = rggb([0.5, 0.25, 0.125]);
    let mut frame = frame(
        RGGB,
        (1368, 912),
        PixelRect::new(12, 24, 1368, 912),
        1,
        |x, y| 0.05 + 0.8 * colour(x, y) * (1.0 + (f64::from(x * y) / 5000.0).sin()) / 2.0,
    );
    let mut meta = frame.meta().clone();
    meta.black_level = 0.05 * 65_535.0;
    frame = Frame::new(frame_buffer(&frame), meta).unwrap();
    let balanced = Look {
        correction: ColourCorrection {
            gains: [1.0, 1.6, 2.4],
            ..ColourCorrection::default()
        },
        ..Look::default()
    };
    for (viewport, roi, look) in [
        ((300, 300), None, Look::default()),
        ((1000, 1000), None, balanced),
        (
            (900, 900),
            Some(PixelRect::new(213, 101, 700, 500)),
            Look::default(),
        ),
        (
            (100, 100),
            Some(PixelRect::new(500, 400, 360, 240)),
            balanced,
        ),
    ] {
        let request = request(viewport, roi);
        let cpu = preview.render(&frame, &request, &look, false).unwrap();
        let gpu = preview.render(&frame, &request, &look, true).unwrap();
        assert!(gpu.gpu && !cpu.gpu);
        assert_eq!(cpu.jpeg, gpu.jpeg, "{viewport:?} {roi:?}");
    }
}
