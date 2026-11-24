//! The calibration in the preview (W8.2): the dark frame and the flat field
//! taken off the raw mosaic as the calibration measured them, `(raw − dark)
//! / flat`, and the colour calibration as the correction; each only when the
//! look asks for it, and an image that doesn't cover the frame not used.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use oscope_acquire::preview::calibrate::correct;
use oscope_acquire::preview::{
    Calibrated, Calibrating, Calibration, Look, Preview, PreviewRequest, Wanted,
};
use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_core::{
    BayerPattern, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, PixelFormat, PixelRect,
    Timestamp,
};
use oscope_imaging::color::ColourCorrection;

const RGGB: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};
/// The frame's size, and where it is on the sensor.
const SIZE: (u32, u32) = (64, 48);
const AT: (u32, u32) = (100, 40);
/// The camera's black level, which the dark frame holds too.
const BLACK: f64 = 0.05;

/// The sample's own colour, an RGGB mosaic of one colour.
fn signal(x: u32, y: u32) -> f64 {
    match (x % 2, y % 2) {
        (0, 0) => 0.30,
        (1, 1) => 0.20,
        _ => 0.40,
    }
}

/// The objective's vignetting: brighter in the middle, mean about 1.
fn vignette(x: u32, y: u32) -> f64 {
    let (dx, dy) = ((f64::from(x) - 31.5) / 32.0, (f64::from(y) - 23.5) / 24.0);
    1.15 - 0.3 * (dx * dx + dy * dy)
}

/// The dark frame: the black level, and a fixed pattern on it.
fn dark(x: u32, y: u32) -> f64 {
    BLACK + 0.01 * f64::from((x * 7 + y * 13) % 5)
}

/// A frame of `value(x, y)` at `AT` on the sensor, its black level `BLACK`.
fn frame(value: impl Fn(u32, u32) -> f64) -> Frame {
    framed(value, 1)
}

/// `frame`, its sensor's pixels `binning` × `binning` to each of its own:
/// `AT` and `SIZE` in its own pixels, its region on the sensor `binning`
/// times them.
fn framed(value: impl Fn(u32, u32) -> f64, binning: u32) -> Frame {
    let (width, height) = SIZE;
    let pool = FramePool::new(RGGB.frame_bytes(width, height), 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    for (i, sample) in buffer.bytes_mut().chunks_exact_mut(2).enumerate() {
        let v = value(i as u32 % width, i as u32 / width).clamp(0.0, 1.0);
        sample.copy_from_slice(&((v * 65_535.0).round() as u16).to_le_bytes());
    }
    let mut meta = FrameMeta::new(FrameId(1), RGGB, width, height, Timestamp(0));
    meta.binning = binning;
    meta.black_level = BLACK * 65_535.0;
    meta.roi = PixelRect {
        x: AT.0 * binning,
        y: AT.1 * binning,
        width: width * binning,
        height: height * binning,
    };
    Frame::new(buffer, meta).unwrap()
}

/// An image of `value(x, y)` for the `width` × `height` pixels from `(x, y)` on the sensor.
fn calibrated(
    (x, y): (u32, u32),
    (width, height): (u32, u32),
    value: impl Fn(u32, u32) -> f64,
) -> Calibrated {
    // In the frame's pixels: 0 where the frame isn't.
    let at = |i: u32| {
        let u = i64::from(x + i % width) - i64::from(AT.0);
        let v = i64::from(y + i / width) - i64::from(AT.1);
        let inside = (0..i64::from(SIZE.0)).contains(&u) && (0..i64::from(SIZE.1)).contains(&v);
        if inside {
            value(u as u32, v as u32) as f32
        } else {
            0.0
        }
    };
    let data = (0..width * height).map(at).collect();
    let image = Image::from_vec(Shape::new(width, height, 1), data).unwrap();
    Calibrated { image, x, y }
}

/// The dark frame and the flat field, each over exactly the frame.
fn darkness() -> Calibrated {
    calibrated(AT, SIZE, dark)
}
fn flatness() -> Calibrated {
    calibrated(AT, SIZE, vignette)
}

#[test]
fn the_dark_frame_and_the_flat_field_come_off_as_the_calibration_measured_them() {
    let raw = |x: u32, y: u32| (dark(x, y) + signal(x, y) * vignette(x, y)) as f32;
    let image = || {
        let data = (0..SIZE.0 * SIZE.1)
            .map(|i| raw(i % SIZE.0, i / SIZE.0))
            .collect();
        Image::from_vec(Shape::new(SIZE.0, SIZE.1, 1), data).unwrap()
    };
    let close = |got: &Image, want: &dyn Fn(u32, u32) -> f64| {
        for (i, v) in got.data().iter().enumerate() {
            let (x, y) = (i as u32 % SIZE.0, i as u32 / SIZE.0);
            assert!(
                (f64::from(*v) - want(x, y)).abs() < 1e-5,
                "({x}, {y}): {v} against {}",
                want(x, y)
            );
        }
    };
    // On the black level's scale: full scale less it stretched to 1.
    let full = |v: f64| v / (1.0 - BLACK);
    // Both: the sample's own colour.
    let mut both = image();
    assert!(correct(
        &mut both,
        AT,
        BLACK as f32,
        Some(&darkness()),
        Some(&flatness())
    ));
    close(&both, &|x, y| full(signal(x, y)));
    // The dark frame alone: the vignetting stays.
    let mut dark_only = image();
    assert!(correct(
        &mut dark_only,
        AT,
        BLACK as f32,
        Some(&darkness()),
        None
    ));
    close(&dark_only, &|x, y| full(signal(x, y) * vignette(x, y)));
    // The flat field alone: the black level comes off in the dark frame's place.
    let mut flat_only = image();
    assert!(correct(
        &mut flat_only,
        AT,
        BLACK as f32,
        None,
        Some(&flatness())
    ));
    close(&flat_only, &|x, y| {
        full((dark(x, y) - BLACK) / vignette(x, y) + signal(x, y))
    });
    // A part of a larger image is taken where the frame is on the sensor.
    let wider = calibrated((AT.0 - 8, AT.1 - 4), (SIZE.0 + 16, SIZE.1 + 8), dark);
    let mut from_wider = image();
    assert!(correct(
        &mut from_wider,
        AT,
        BLACK as f32,
        Some(&wider),
        Some(&flatness())
    ));
    close(&from_wider, &|x, y| full(signal(x, y)));
}

#[test]
fn an_image_that_doesnt_cover_the_frame_isnt_used() {
    let image = || {
        Image::from_vec(
            Shape::new(SIZE.0, SIZE.1, 1),
            vec![0.5; (SIZE.0 * SIZE.1) as usize],
        )
        .unwrap()
    };
    // One pixel short on the right; and one of three channels for a mosaic of one.
    let short = calibrated(AT, (SIZE.0 - 1, SIZE.1), dark);
    let elsewhere = calibrated((AT.0 + 1, AT.1), SIZE, dark);
    let colours = Calibrated {
        image: Image::from_vec(
            Shape::new(SIZE.0, SIZE.1, 3),
            vec![0.1; (SIZE.0 * SIZE.1 * 3) as usize],
        )
        .unwrap(),
        x: AT.0,
        y: AT.1,
    };
    for wrong in [short, elsewhere, colours] {
        let mut kept = image();
        assert!(!correct(
            &mut kept,
            AT,
            BLACK as f32,
            Some(&wrong),
            Some(&wrong)
        ));
        assert!(kept.data().iter().all(|&v| v == 0.5));
    }
}

fn preview() -> Preview {
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(1),
        cuda_device: None,
    };
    Preview::new(Arc::new(Compute::new(&config).unwrap()))
}

/// The preview's 8-bit RGB of `frame`, at its own size.
fn shown(preview: &Preview, frame: &Frame, look: &Look) -> Vec<u8> {
    let request = PreviewRequest {
        viewport: (SIZE.0 * 2, SIZE.1 * 2),
        roi: None,
        quality: 95,
    };
    let shown = preview.display(frame, &request, look, false).unwrap();
    assert_eq!((shown.width, shown.height), SIZE, "at its own size");
    shown.rgb
}

/// The largest difference between two pictures' samples.
fn largest(a: &[u8], b: &[u8]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap()
}

#[test]
fn the_look_takes_each_correction_only_when_it_asks_for_it() {
    let colour = ColourCorrection {
        gains: [1.5, 1.0, 2.0],
        matrix: [[1.2, -0.1, -0.1], [-0.05, 1.1, -0.05], [0.0, -0.2, 1.2]],
    };
    let asked = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let preview = preview();
    let (heard, counted) = (Arc::clone(&asked), Arc::clone(&calls));
    preview.set_calibrating(Some(Calibrating(Arc::new(
        move |meta: &FrameMeta, wanted: Wanted| {
            counted.fetch_add(1, Ordering::SeqCst);
            heard.lock().unwrap().push((meta.roi.x, meta.roi.y, wanted));
            Calibration {
                dark: wanted.dark.then(|| Arc::new(darkness())),
                flat: wanted.flat.then(|| Arc::new(flatness())),
                colour: wanted.colour.then_some(colour),
            }
        },
    ))));
    // The sample as the camera sees it: its dark frame on it, vignetted.
    let seen = frame(|x, y| dark(x, y) + signal(x, y) * vignette(x, y));
    // What it should show: the sample alone, as a frame without either.
    let truth = shown(
        &preview,
        &frame(|x, y| BLACK + signal(x, y)),
        &Look::default(),
    );
    let both = Look {
        wanted: Wanted {
            dark: true,
            flat: true,
            colour: false,
        },
        ..Look::default()
    };
    assert!(largest(&shown(&preview, &seen, &both), &truth) <= 1);
    assert_eq!(
        asked.lock().unwrap().last(),
        Some(&(AT.0, AT.1, both.wanted))
    );
    // Off, the calibration isn't asked, and the vignetting and dark frame show.
    let before = calls.load(Ordering::SeqCst);
    let plain = shown(&preview, &seen, &Look::default());
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(largest(&plain, &truth) > 10);
    // The colour calibration is the correction: as if the look had it.
    let coloured = Look {
        wanted: Wanted {
            colour: true,
            ..both.wanted
        },
        ..Look::default()
    };
    let by_hand = Look {
        correction: colour,
        ..both
    };
    assert_eq!(
        shown(&preview, &seen, &coloured),
        shown(&preview, &seen, &by_hand)
    );
    assert!(largest(&shown(&preview, &seen, &coloured), &truth) > 10);
    // Without a way to find it, a look that asks gets the plain picture.
    preview.set_calibrating(None);
    assert_eq!(shown(&preview, &seen, &both), plain);
}

#[test]
fn a_binned_frame_is_matched_to_its_calibration_in_its_own_pixels() {
    let preview = preview();
    preview.set_calibrating(Some(Calibrating(Arc::new(
        |_: &FrameMeta, wanted: Wanted| Calibration {
            dark: wanted.dark.then(|| Arc::new(darkness())),
            flat: wanted.flat.then(|| Arc::new(flatness())),
            colour: None,
        },
    ))));
    let both = Look {
        wanted: Wanted {
            dark: true,
            flat: true,
            colour: false,
        },
        ..Look::default()
    };
    // Binned 2 × 2: its region's corner on the sensor is twice where the
    // images are, which count in the frame's pixels.
    let seen = framed(|x, y| dark(x, y) + signal(x, y) * vignette(x, y), 2);
    let truth = shown(
        &preview,
        &frame(|x, y| BLACK + signal(x, y)),
        &Look::default(),
    );
    assert!(largest(&shown(&preview, &seen, &both), &truth) <= 1);
}
