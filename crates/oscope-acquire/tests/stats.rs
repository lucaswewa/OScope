//! Live statistics against references computed here: histograms,
//! percentiles and saturation, subsampling, ROI statistics, profiles, the
//! saturation mask's packing, and the focus value.

use oscope_acquire::preview::{Preview, PreviewError, Rendered};
use oscope_acquire::stats::{Line, Mask, PERCENTILES, StatsRequest, measure};
use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{
    BayerColour, BayerPattern, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, PixelFormat,
    PixelRect, Timestamp,
};

const RGGB12: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 12,
};

/// A frame of `format`, `width` × `height`, of the sensor's pixels from
/// `origin`, binned by `binning`: each sample `adu(x, y)` in the sensor's
/// units.
fn frame(
    format: PixelFormat,
    (width, height): (u32, u32),
    origin: (u32, u32),
    binning: u32,
    adu: impl Fn(u32, u32) -> u32,
) -> Frame {
    let pool = FramePool::new(format.frame_bytes(width, height), 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    let shift = match format {
        PixelFormat::Bayer16 {
            significant_bits, ..
        } => 16 - u32::from(significant_bits),
        _ => 0,
    };
    let (bytes, channels) = (format.sample_bytes(), format.channels() as u32);
    for (i, sample) in buffer.bytes_mut().chunks_exact_mut(bytes).enumerate() {
        let pixel = i as u32 / channels;
        let v = adu(pixel % width, pixel / width) << shift;
        match bytes {
            2 => sample.copy_from_slice(&(v as u16).to_le_bytes()),
            _ => sample[0] = v as u8,
        }
    }
    let mut meta = FrameMeta::new(FrameId(3), format, width, height, Timestamp(0));
    let (w, h) = (width * binning, height * binning);
    meta.roi = PixelRect::new(origin.0, origin.1, w, h);
    meta.binning = binning;
    Frame::new(buffer, meta).unwrap()
}

/// An RGGB pixel's channel: 0, 1 or 2.
fn channel(x: u32, y: u32) -> usize {
    match BayerPattern::Rggb.colour_at(x, y) {
        BayerColour::Red => 0,
        BayerColour::Green => 1,
        BayerColour::Blue => 2,
    }
}

fn whole(frame: &Frame) -> PixelRect {
    frame.meta().roi
}

fn request() -> StatsRequest {
    StatsRequest {
        bins: 4096,
        ..StatsRequest::default()
    }
}

#[test]
fn histograms_percentiles_and_saturation_match_a_reference() {
    // Every sample counted: a small frame isn't subsampled.
    let adu = |x: u32, y: u32| match (x * 37 + y * 101) % 61 {
        0 => 4095,
        k => (x * 53 + y * 29 + k * 17) % 4095,
    };
    let frame = frame(RGGB12, (64, 48), (0, 0), 1, adu);
    let mut values = vec![Vec::new(); 3];
    let mut reference = vec![vec![0_u64; 4096]; 3];
    for y in 0..48 {
        for x in 0..64 {
            values[channel(x, y)].push(adu(x, y));
            reference[channel(x, y)][adu(x, y) as usize] += 1;
        }
    }
    let (stats, _) = measure(&frame, &request(), whole(&frame), (64, 48)).unwrap();
    assert_eq!(stats.channels, ["red", "green", "blue"]);
    assert_eq!(stats.full_scale, 4095);
    assert_eq!(stats.counted, [768, 1536, 768]);
    assert_eq!(stats.histogram, reference);
    for (c, values) in values.iter_mut().enumerate() {
        values.sort_unstable();
        let n = values.len() as f64;
        let expected: Vec<f64> = PERCENTILES
            .iter()
            .map(|p| f64::from(values[((p / 100.0 * n).ceil().max(1.0) - 1.0) as usize]))
            .collect();
        assert_eq!(stats.percentiles[c], expected, "channel {c}");
        let saturated = values.iter().filter(|&&v| v == 4095).count() as f64;
        assert_eq!(stats.saturated[c], saturated / n, "channel {c}");
    }

    // 256 bins of 16 levels each.
    let coarse = StatsRequest::default();
    let (stats, _) = measure(&frame, &coarse, whole(&frame), (64, 48)).unwrap();
    for (c, bins) in stats.histogram.iter().enumerate() {
        let summed: Vec<u64> = reference[c].chunks(16).map(|b| b.iter().sum()).collect();
        assert_eq!(bins, &summed, "channel {c}");
    }
}

#[test]
fn large_frames_are_subsampled_by_whole_cells() {
    // 2048 × 1024, a ramp along x from 0 to 4094: subsampled to about a
    // million samples, every colour in proportion, the median in the
    // middle.
    let frame = frame(RGGB12, (2048, 1024), (0, 0), 1, |x, _| 2 * x);
    let (stats, _) = measure(&frame, &StatsRequest::default(), whole(&frame), (512, 256)).unwrap();
    let total: u64 = stats.counted.iter().sum();
    assert!((1 << 19..=1 << 20).contains(&total), "{total}");
    assert_eq!(stats.counted[1], 2 * stats.counted[0]);
    assert_eq!(stats.counted[0], stats.counted[2]);
    for c in 0..3 {
        let median = stats.percentiles[c][2];
        assert!((median - 2047.0).abs() < 41.0, "channel {c}: {median}");
    }
}

#[test]
fn roi_statistics_are_each_channel_s() {
    // A frame of the sensor's pixels from (100, 50); the ROI from (110, 60).
    let adu = |x: u32, y: u32| (x * x + 3 * y) % 4000;
    let frame = frame(RGGB12, (64, 48), (100, 50), 1, adu);
    let roi = PixelRect::new(110, 60, 20, 10);
    let asked = StatsRequest {
        roi: Some(roi),
        ..request()
    };
    let (stats, _) = measure(&frame, &asked, whole(&frame), (64, 48)).unwrap();
    let measured = stats.roi.unwrap();
    assert_eq!(measured.rect, roi);
    let mut values = vec![Vec::new(); 3];
    for y in 10..20 {
        for x in 10..30 {
            values[channel(x, y)].push(f64::from(adu(x, y)));
        }
    }
    for (c, values) in values.iter().enumerate() {
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let std = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
        assert_eq!(measured.count[c], values.len() as u64);
        assert!((measured.mean[c] - mean).abs() < 1e-9, "channel {c}");
        assert!((measured.std[c] - std).abs() < 1e-6, "channel {c}");
        let (min, max) = values
            .iter()
            .fold((f64::MAX, 0.0_f64), |(a, b), &v| (a.min(v), b.max(v)));
        assert_eq!(
            (f64::from(measured.min[c]), f64::from(measured.max[c])),
            (min, max)
        );
    }

    // Widened to whole cells, clipped to the frame; refused when it misses.
    let edge = StatsRequest {
        roi: Some(PixelRect::new(161, 91, 40, 40)),
        ..request()
    };
    let (stats, _) = measure(&frame, &edge, whole(&frame), (64, 48)).unwrap();
    assert_eq!(stats.roi.unwrap().rect, PixelRect::new(160, 90, 4, 8));
    let off = StatsRequest {
        roi: Some(PixelRect::new(0, 0, 50, 50)),
        ..request()
    };
    let missed = measure(&frame, &off, whole(&frame), (64, 48));
    assert!(matches!(missed, Err(PreviewError::Roi { .. })));
}

#[test]
fn profiles_sample_cell_by_cell() {
    // Red counts the cells across, green the cells down; blue is fixed.
    let adu = |x: u32, y: u32| match channel(x, y) {
        0 => 100 + x / 2,
        1 => 200 + y / 2,
        _ => 300,
    };
    let frame = frame(RGGB12, (64, 48), (100, 50), 1, adu);
    // Along a row, from the frame's left edge to 10 sensor pixels past its
    // right one: a sample a cell, none off the frame.
    let line = Line {
        from: [100.0, 61.0],
        to: [174.0, 61.0],
    };
    let asked = StatsRequest {
        profile: Some(line),
        ..request()
    };
    let (stats, _) = measure(&frame, &asked, whole(&frame), (64, 48)).unwrap();
    let profile = stats.profile.unwrap();
    assert_eq!(profile.line, line);
    assert_eq!(profile.samples.len(), 38);
    assert!((profile.step - 2.0).abs() < 1e-9);
    for (i, sample) in profile.samples.iter().enumerate() {
        match sample {
            Some(values) => assert_eq!(values, &[100.0 + i as f64, 205.0, 300.0], "sample {i}"),
            None => assert!(i >= 32, "sample {i}"),
        }
    }
    assert_eq!(profile.samples.iter().filter(|s| s.is_none()).count(), 6);

    // Samples on cells' edges land on them, at any length.
    let ramp = self::frame(RGGB12, (1024, 4), (0, 0), 1, adu);
    for length in [20.0, 58.0, 100.0, 202.0, 998.0] {
        let along = StatsRequest {
            profile: Some(Line {
                from: [0.0, 1.0],
                to: [length, 1.0],
            }),
            ..request()
        };
        let (stats, _) = measure(&ramp, &along, whole(&ramp), (512, 2)).unwrap();
        let samples = stats.profile.unwrap().samples;
        let reds: Vec<f64> = samples.iter().map(|s| s.as_ref().unwrap()[0]).collect();
        let cells: Vec<f64> = (0..reds.len()).map(|i| 100.0 + i as f64).collect();
        assert_eq!(reds, cells, "{length}");
    }

    // A binned frame's cells are twice as far apart on the sensor.
    let binned = self::frame(RGGB12, (32, 24), (0, 0), 2, adu);
    let down = Line {
        from: [1.0, 0.0],
        to: [1.0, 40.0],
    };
    let asked = StatsRequest {
        profile: Some(down),
        ..request()
    };
    let (stats, _) = measure(&binned, &asked, whole(&binned), (32, 24)).unwrap();
    let profile = stats.profile.unwrap();
    assert_eq!(profile.samples.len(), 11);
    let greens: Vec<f64> = profile
        .samples
        .iter()
        .map(|s| s.as_ref().unwrap()[1])
        .collect();
    assert_eq!(
        greens,
        (0..11).map(|i| 200.0 + f64::from(i)).collect::<Vec<_>>()
    );
}

#[test]
fn the_mask_marks_saturated_pixels_at_the_preview_s_resolution() {
    let saturated = [(10, 6), (63, 47), (40, 20)];
    let frame = frame(RGGB12, (64, 48), (0, 0), 1, |x, y| {
        if saturated.contains(&(x, y)) {
            4095
        } else {
            1000
        }
    });
    // A quarter the size each way, 16 × 12: two bytes a row.
    let (_, mask) = measure(&frame, &request(), whole(&frame), (16, 12)).unwrap();
    let mask = mask.unwrap();
    assert_eq!((mask.width, mask.height, mask.bits.len()), (16, 12, 24));
    let set: Vec<(u32, u32)> = (0..12)
        .flat_map(|y| (0..16).map(move |x| (x, y)))
        .filter(|&(x, y)| mask.get(x, y))
        .collect();
    assert_eq!(set, [(2, 1), (10, 5), (15, 11)]);
    // The first pixel in each byte's highest bit.
    assert_eq!(mask.bits[2], 0x80 >> 2);
    assert_eq!(mask.bits[23], 0x01);

    // Rows are padded to whole bytes; the block's bytes read back.
    let (_, mask) = measure(&frame, &request(), whole(&frame), (13, 10)).unwrap();
    let mask = mask.unwrap();
    assert_eq!(mask.bits.len(), 2 * 10);
    let bytes = mask.to_bytes();
    assert_eq!(bytes[..8], [13, 0, 0, 0, 10, 0, 0, 0]);
    assert_eq!(Mask::from_bytes(&bytes), Some(mask));
    assert_eq!(Mask::from_bytes(&bytes[..bytes.len() - 1]), None);

    // Only the region shown: from (8, 4), 32 × 24, at 8 × 6.
    let shown = PixelRect::new(8, 4, 32, 24);
    let (_, mask) = measure(&frame, &request(), shown, (8, 6)).unwrap();
    let mask = mask.unwrap();
    assert!(mask.get(0, 0));
    assert_eq!(mask.bits.iter().map(|b| b.count_ones()).sum::<u32>(), 1);

    // None when nothing is at full scale, or when not asked for.
    let unsaturated = self::frame(RGGB12, (64, 48), (0, 0), 1, |_, _| 4094);
    assert_eq!(
        measure(&unsaturated, &request(), whole(&unsaturated), (16, 12))
            .unwrap()
            .1,
        None
    );
    let unasked = StatsRequest {
        mask: false,
        ..request()
    };
    assert_eq!(
        measure(&frame, &unasked, whole(&frame), (16, 12))
            .unwrap()
            .1,
        None
    );
}

#[test]
fn the_focus_value_falls_as_the_image_blurs() {
    // Bars 8 pixels wide, then the same bars averaged over wider windows:
    // the same mean, less contrast.
    let bars = |x: i64| if (x / 8) % 2 == 0 { 3000.0 } else { 1000.0 };
    let blurred = |k: i64| {
        move |x: u32, _: u32| {
            let x = i64::from(x);
            ((x - k..=x + k).map(bars).sum::<f64>() / (2 * k + 1) as f64) as u32
        }
    };
    let focus: Vec<f64> = [0, 2, 4, 8]
        .into_iter()
        .map(|k| {
            let frame = frame(RGGB12, (128, 64), (0, 0), 1, blurred(k));
            measure(&frame, &StatsRequest::default(), whole(&frame), (64, 32))
                .unwrap()
                .0
                .focus
        })
        .collect();
    assert!(focus.windows(2).all(|w| w[0] > w[1]), "{focus:?}");
    // Normalised by the mean: twice the light, twice the value.
    let at = |gain: u32| {
        let frame = frame(RGGB12, (128, 64), (0, 0), 1, |x, y| {
            gain * blurred(2)(x, y) / 2
        });
        measure(&frame, &StatsRequest::default(), whole(&frame), (64, 32))
            .unwrap()
            .0
            .focus
    };
    let ratio = at(2) / at(1);
    assert!((ratio - 2.0).abs() < 0.01, "{ratio}");
    // A flat frame has none.
    let flat = frame(RGGB12, (64, 48), (0, 0), 1, |_, _| 1500);
    assert_eq!(
        measure(&flat, &request(), whole(&flat), (32, 24))
            .unwrap()
            .0
            .focus,
        0.0
    );
}

#[test]
fn other_formats_are_measured_in_their_own_units() {
    // Grey, 16 bits: one channel up to 65535, counted at 4096 levels.
    let grey = frame(PixelFormat::Mono16, (32, 16), (0, 0), 1, |x, _| x * 2048);
    let (stats, _) = measure(&grey, &request(), whole(&grey), (32, 16)).unwrap();
    assert_eq!(
        (stats.channels.clone(), stats.full_scale),
        (vec!["grey".to_owned()], 65_535)
    );
    assert_eq!(stats.histogram[0].len(), 4096);
    assert_eq!(
        stats.histogram[0][128], 16,
        "x = 1, 2048 in 16-bit units, is level 128"
    );
    // RGB, 8 bits: three channels up to 255, at 256 levels.
    let rgb = frame(PixelFormat::Rgb8, (8, 8), (0, 0), 1, |_, _| 255);
    let (stats, mask) = measure(&rgb, &request(), whole(&rgb), (4, 4)).unwrap();
    assert_eq!((stats.full_scale, stats.histogram[2].len()), (255, 256));
    assert_eq!(stats.saturated, [1.0; 3]);
    assert_eq!(mask.unwrap().bits, [0xf0; 4]);
    // Not one plane of samples: refused.
    let format = PixelFormat::PlanarF32 { planes: 2 };
    let pool = FramePool::new(format.frame_bytes(8, 8), 1, &HeapAllocator);
    let meta = FrameMeta::new(FrameId(1), format, 8, 8, Timestamp(0));
    let planar = Frame::new(pool.take().unwrap(), meta).unwrap();
    let refused = measure(&planar, &request(), PixelRect::new(0, 0, 8, 8), (8, 8));
    assert_eq!(refused.unwrap_err(), PreviewError::Format(format));
}

/// What a 20 MP frame's statistics cost in the live pool, as a session
/// measures them, with an ROI, a profile and the mask, for a 1920 px
/// preview: in release, `cargo test --release -p oscope-acquire --test
/// stats -- --ignored --nocapture`.
#[test]
#[ignore = "a benchmark: run it in release"]
fn the_cost_at_20_mp() {
    let frame = frame(RGGB12, (5472, 3648), (0, 0), 1, |x, y| {
        (x * 7 + y * 13) % 4096
    });
    let asked = StatsRequest {
        roi: Some(PixelRect::new(2000, 1500, 1000, 600)),
        profile: Some(Line {
            from: [0.0, 1824.0],
            to: [5471.0, 1824.0],
        }),
        ..StatsRequest::default()
    };
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    let preview = Preview::new(std::sync::Arc::new(compute));
    let rendered = Rendered {
        jpeg: Vec::new(),
        roi: whole(&frame),
        width: 1920,
        height: 1280,
        gpu: false,
        took: std::time::Duration::ZERO,
        recipe: None,
    };
    preview.measure(&frame, &asked, &rendered).unwrap();
    let mut took: Vec<_> = (0..9)
        .map(|_| {
            let started = std::time::Instant::now();
            preview.measure(&frame, &asked, &rendered).unwrap();
            started.elapsed()
        })
        .collect();
    took.sort();
    println!("statistics of 20 MP: {:.1} ms", took[4].as_secs_f64() * 1e3);
}
