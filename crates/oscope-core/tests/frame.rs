//! Frames: the pool, sealing, sharing, views, and metadata.

use std::time::Duration;

use oscope_core::{
    BayerPattern, Error, Frame, FrameId, FrameMeta, FramePool, FrameView, HeapAllocator, Ms,
    PixelFormat, PixelRect, Timestamp,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const RGGB16: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};

fn meta(id: u64, format: PixelFormat, width: u32, height: u32) -> FrameMeta {
    FrameMeta::new(FrameId(id), format, width, height, Timestamp(id * 1_000))
}

#[test]
fn an_exhausted_pool_counts_the_lost_frame_and_reuses_returned_slabs() {
    let pool = FramePool::new(64, 2, &HeapAllocator);
    let first = pool.take().unwrap();
    let second = pool.take().unwrap();
    assert!(pool.take().is_none(), "both slabs are in use");
    assert_eq!((pool.stats().in_use, pool.stats().exhausted), (2, 1));
    drop(first);
    let frame = Frame::new(second, meta(0, PixelFormat::Mono8, 8, 8)).unwrap();
    let again = pool
        .take()
        .expect("the dropped buffer went back to the pool");
    assert_eq!(pool.stats().in_use, 2);
    let clone = frame.clone();
    drop(frame);
    assert_eq!(pool.stats().in_use, 2, "a clone still holds the slab");
    drop((clone, again));
    assert_eq!(pool.stats().in_use, 0);
}

#[test]
fn clones_share_one_slab() {
    let pool = FramePool::new(32, 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    buffer.bytes_mut().fill(7);
    let frame = Frame::new(buffer, meta(3, PixelFormat::Mono8, 4, 4)).unwrap();
    let clone = frame.clone();
    assert!(frame.same_pixels(&clone));
    assert!(std::ptr::eq(frame.bytes(), clone.bytes()));
    assert_eq!(
        frame.bytes(),
        [7; 16],
        "exactly the frame's bytes, not the whole slab"
    );
    let other = Frame::new(
        FramePool::new(16, 1, &HeapAllocator).take().unwrap(),
        meta(4, PixelFormat::Mono8, 4, 4),
    );
    assert!(!frame.same_pixels(&other.unwrap()));
}

#[test]
fn a_buffer_too_small_for_its_frame_is_refused() {
    let buffer = FramePool::new(100, 1, &HeapAllocator).take().unwrap();
    let error = Frame::new(buffer, meta(0, RGGB16, 10, 10)).unwrap_err();
    assert!(matches!(
        error,
        Error::BufferTooSmall {
            needed: 200,
            got: 100
        }
    ));
}

#[test]
fn formats_know_their_sizes() {
    assert_eq!(
        RGGB16.frame_bytes(5472, 3648),
        39_923_712,
        "a 20 MP mosaic is 40 MB"
    );
    assert_eq!(PixelFormat::Rgb16.frame_bytes(2, 2), 24);
    assert_eq!(PixelFormat::PlanarF32 { planes: 3 }.frame_bytes(2, 2), 48);
    assert_eq!(
        PixelFormat::Rgb16.frame_bytes(u32::MAX, u32::MAX),
        usize::MAX,
        "saturates rather than overflowing"
    );
}

#[test]
fn a_planar_frame_views_each_plane() {
    let format = PixelFormat::PlanarF32 { planes: 2 };
    let mut buffer = FramePool::new(32, 1, &HeapAllocator).take().unwrap();
    buffer.bytes_mut()[16..].fill(1);
    let frame = Frame::new(buffer, meta(0, format, 2, 2)).unwrap();
    assert_eq!(frame.plane(0).unwrap().row(0).unwrap(), [0; 8]);
    assert_eq!(frame.plane(1).unwrap().row(1).unwrap(), [1; 8]);
    assert!(frame.plane(2).is_none());
}

/// Pixels whose samples encode their own position, so a view's reading can
/// be checked against where it came from.
fn image(format: PixelFormat, width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            for c in 0..format.channels() as u32 {
                let value = (y * 251 + x * 7 + c) as u16;
                match format.sample_bytes() {
                    1 => data.push(value as u8),
                    _ => data.extend(value.to_le_bytes()),
                }
            }
        }
    }
    data
}

fn random_rect(rng: &mut StdRng, width: u32, height: u32) -> PixelRect {
    let (w, h) = (rng.random_range(1..=width), rng.random_range(1..=height));
    PixelRect::new(
        rng.random_range(0..=width - w),
        rng.random_range(0..=height - h),
        w,
        h,
    )
}

#[test]
fn a_view_of_a_rectangle_reads_the_same_pixels_as_its_parent() {
    let formats = [
        RGGB16,
        PixelFormat::Bayer8 {
            pattern: BayerPattern::Gbrg,
        },
        PixelFormat::Mono8,
        PixelFormat::Rgb8,
        PixelFormat::Rgb16,
        PixelFormat::Rgba8,
    ];
    for case in 0..300 {
        let mut rng = StdRng::seed_from_u64(case);
        let format = formats[rng.random_range(0..formats.len())];
        let (width, height) = (rng.random_range(1..40), rng.random_range(1..40));
        let data = image(format, width, height);
        let parent = FrameView::new(&data, format, width, height).unwrap();
        let outer = random_rect(&mut rng, width, height);
        let view = parent.roi(outer).unwrap();
        let inner = random_rect(&mut rng, outer.width, outer.height);
        let nested = view.roi(inner).unwrap();
        let direct = parent
            .roi(PixelRect::new(
                outer.x + inner.x,
                outer.y + inner.y,
                inner.width,
                inner.height,
            ))
            .unwrap();
        for y in 0..inner.height {
            for x in 0..inner.width {
                for c in 0..format.channels() {
                    let here = nested.sample(x, y, c);
                    assert_eq!(
                        here,
                        parent.sample(outer.x + inner.x + x, outer.y + inner.y + y, c),
                        "case {case}"
                    );
                    assert_eq!(here, direct.sample(x, y, c), "case {case}");
                }
                if let (Some(pattern), Some(original)) = (nested.format().bayer(), format.bayer()) {
                    let colour = original.colour_at(outer.x + inner.x + x, outer.y + inner.y + y);
                    assert_eq!(
                        pattern.colour_at(x, y),
                        colour,
                        "case {case}: the mosaic's colours follow the offset"
                    );
                }
            }
        }
        assert_eq!(nested.format(), direct.format(), "case {case}");
        assert_eq!(
            nested.sample(inner.width, 0, 0),
            None,
            "case {case}: outside"
        );
    }
}

#[test]
fn a_rectangle_must_be_non_empty_and_inside() {
    let data = image(PixelFormat::Mono8, 10, 10);
    let view = FrameView::new(&data, PixelFormat::Mono8, 10, 10).unwrap();
    for rect in [
        PixelRect::new(0, 0, 0, 5),
        PixelRect::new(5, 5, 6, 1),
        PixelRect::new(10, 0, 1, 1),
        PixelRect::new(0, u32::MAX, 1, 1),
    ] {
        assert!(
            matches!(view.roi(rect), Err(Error::OutsideFrame { .. })),
            "{rect:?}"
        );
    }
    assert!(view.roi(PixelRect::new(9, 9, 1, 1)).is_ok());
    assert!(
        FrameView::new(&data, PixelFormat::Mono16, 10, 10).is_err(),
        "too few bytes"
    );
}

#[test]
fn a_views_data_runs_from_its_first_pixel_to_its_last() {
    let data = image(PixelFormat::Rgb16, 10, 8);
    let view = FrameView::new(&data, PixelFormat::Rgb16, 10, 8).unwrap();
    assert_eq!(view.data(), &data[..]);
    let part = view.roi(PixelRect::new(2, 3, 4, 2)).unwrap();
    // Two rows of 4 pixels, 60 bytes apart, starting at (2, 3).
    let start = 3 * 60 + 2 * 6;
    assert_eq!(part.data(), &data[start..start + 60 + 4 * 6]);
    assert_eq!(part.data()[..24], *part.row(0).unwrap());
}

#[test]
fn a_rolling_shutter_exposes_each_row_a_line_time_later() {
    let mut meta = meta(0, RGGB16, 5472, 3648);
    meta.exposure_start = Timestamp(1_000_000_000);
    meta.exposure = Ms(5.0);
    assert_eq!(
        meta.exposure_midpoint(),
        Timestamp(1_002_500_000),
        "a global shutter"
    );
    meta.line_time = Some(Ms(0.016));
    let (start, end) = meta.row_exposure(3647);
    assert_eq!(
        start,
        Timestamp(1_000_000_000) + Duration::from_micros(58_352)
    );
    assert_eq!(end.checked_since(start), Some(Duration::from_millis(5)));
    assert_eq!(
        meta.exposure_midpoint(),
        Timestamp(1_000_000_000 + 29_184_000 + 2_500_000),
        "the middle row's"
    );
}

#[test]
fn metadata_and_formats_serialise_as_json() {
    let mut meta = meta(9, RGGB16, 5472, 3648);
    meta.line_time = Some(Ms(0.016));
    let json = serde_json::to_value(&meta).unwrap();
    assert_eq!(
        json["format"],
        serde_json::json!({"kind": "bayer16", "pattern": "RGGB", "significant_bits": 16})
    );
    assert_eq!(
        json["roi"],
        serde_json::json!({"x": 0, "y": 0, "width": 5472, "height": 3648})
    );
    assert_eq!(json["sensor_time"], serde_json::Value::Null);
    assert_eq!(serde_json::from_value::<FrameMeta>(json).unwrap(), meta);
    assert_eq!(
        serde_json::to_value(PixelFormat::Mono8).unwrap(),
        serde_json::json!({"kind": "mono8"})
    );
}
