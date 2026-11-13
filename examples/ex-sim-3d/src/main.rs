//! `ex-sim-3d`: the simulator's 3D samples, stepped through in z, with
//! their ground truth. It renders a z-series of the reflective surface at
//! 20×, with its true height map and what depth from focus finds; then a
//! z-series of the fluorescent cells, in widefield and through a confocal,
//! with slices of their true volume. It writes the images as PNGs and the
//! truths as raw files, and serves a page that steps through z. See the
//! README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::{Display, encode_png, to_display};
use oscope_sim::cells::{DENSITY, DYES};
use oscope_sim::fluorescence::optical_section;
use oscope_sim::{CellCulture, Excitation, Field, Light, Optics, Slide, Surface, World};
use serde_json::{Value, json};

/// The stage z at which the slide's surface is in focus. The slide's corner
/// is at the stage's origin, so stage and slide coordinates agree.
const FOCUS: f64 = 5_000.0;
/// The chip's corner, and the culture, on the slide.
const CHIP: Point2 = Point2::new(30_000.0, 8_000.0);
const CULTURE: Rect = Rect::new(40_000.0, 5_000.0, 10_000.0, 10_000.0);
/// The lines that excite the culture's three dyes.
const LINES: [Excitation; 3] = [Excitation::Violet, Excitation::Blue, Excitation::Green];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-3d: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// An image through a display transform from black to `white`, as a PNG
/// file's bytes.
fn png(image: &Image, white: f32) -> anyhow::Result<Vec<u8>> {
    let display = Display {
        white,
        ..Display::default()
    };
    let shown = to_display(image, &display)?;
    let mut file = Vec::new();
    encode_png(&mut file, &shown, image.width(), image.height())?;
    Ok(file)
}

/// Heights from `lo` to `hi` in false colour, from blue through green to
/// yellow; NaN as black.
fn heights_image(map: &[f32], lo: f64, hi: f64, width: u32, height: u32) -> Image {
    let data = map
        .iter()
        .flat_map(|h| {
            if h.is_nan() {
                return [0.0; 3];
            }
            let t = ((f64::from(*h) - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
            [t * t, 0.15 + 0.75 * t, 0.9 * (1.0 - t)]
        })
        .collect();
    Image::from_vec(Shape::new(width, height, 3), data).expect("the map's size")
}

/// Depth from focus, for each pixel: the z of its sharpest image, to a
/// parabola through its neighbours. Sharpness is the modified Laplacian in
/// green with neighbours `k` pixels away (Nayar and Nakagawa, 1994), summed
/// over a square `window` pixels across.
fn depth_from_focus(stack: &[Image], zs: &[f64], k: usize, window: usize) -> Vec<f32> {
    let (w, h) = (stack[0].width() as usize, stack[0].height() as usize);
    let measures: Vec<Vec<f64>> = stack
        .iter()
        .map(|image| {
            let g = |x: usize, y: usize| f64::from(image.data()[(y * w + x) * 3 + 1]);
            let mut ml = vec![0.0; w * h];
            for y in k..h.saturating_sub(k) {
                for x in k..w.saturating_sub(k) {
                    let lx = (2.0 * g(x, y) - g(x - k, y) - g(x + k, y)).abs();
                    let ly = (2.0 * g(x, y) - g(x, y - k) - g(x, y + k)).abs();
                    ml[y * w + x] = (lx + ly).powi(2);
                }
            }
            window_sums(&ml, w, h, window / 2)
        })
        .collect();
    let step = zs[1] - zs[0];
    (0..w * h)
        .map(|i| {
            let best = (1..zs.len() - 1)
                .max_by(|a, b| measures[*a][i].total_cmp(&measures[*b][i]))
                .unwrap_or(1);
            let (m, c, q) = (
                measures[best - 1][i],
                measures[best][i],
                measures[best + 1][i],
            );
            let curve = m - 2.0 * c + q;
            let offset = if curve < 0.0 {
                0.5 * (m - q) / curve
            } else {
                0.0
            };
            (zs[best] + offset.clamp(-1.0, 1.0) * step) as f32
        })
        .collect()
}

/// The sum over the square of `r` pixels around each pixel, from an
/// integral image.
fn window_sums(values: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> {
    let mut integral = vec![0.0; (w + 1) * (h + 1)];
    for y in 0..h {
        for x in 0..w {
            integral[(y + 1) * (w + 1) + x + 1] =
                values[y * w + x] + integral[y * (w + 1) + x + 1] + integral[(y + 1) * (w + 1) + x]
                    - integral[y * (w + 1) + x];
        }
    }
    let at = |x: usize, y: usize| integral[y * (w + 1) + x];
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let (x0, y0, x1, y1) = (
                x.saturating_sub(r),
                y.saturating_sub(r),
                (x + r + 1).min(w),
                (y + r + 1).min(h),
            );
            at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0)
        })
        .collect()
}

fn raw(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// The surface: a z-series at 20× over a die's middle, its true height map,
/// and depth from focus against it.
fn surface(page: &Path, seed: u64, smoke: bool) -> anyhow::Result<Value> {
    let surface = Surface::new(seed, CHIP);
    let world = World::with_slide(
        Slide::new(Arc::new(surface.clone()), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("20x")?, Um(2.4));
    let p = optics.pixel_size().0;
    let (w, h) = if smoke { (192, 128) } else { (768, 512) };
    let centre = surface.die_centre(Point2::new(CHIP.x + 2_500.0, CHIP.y + 2_500.0));
    let origin = Point2::new(
        centre.x - f64::from(w) * p / 2.0,
        centre.y - f64::from(h) * p / 2.0,
    );
    let truth = surface.height_map(origin, Um(p), w, h);
    let (lo, hi) = truth.iter().fold((f64::MAX, f64::MIN), |(l, h), v| {
        (l.min(f64::from(*v)), h.max(f64::from(*v)))
    });

    let zs: Vec<f64> = (0..=16).map(|i| f64::from(i) * 0.5).collect();
    let started = Instant::now();
    let mut stack = Vec::new();
    let mut planes = Vec::new();
    for (i, z) in zs.iter().enumerate() {
        let field = Field {
            centre,
            z: Um(FOCUS + z),
            width: w,
            height: h,
        };
        let image = world.render(&field, &optics, Light::Reflected);
        let file = format!("surface-{i}.png");
        std::fs::write(page.join(&file), png(&image, 1.0)?)?;
        planes.push(json!({"z": z, "file": file}));
        stack.push(image);
    }
    let ms = started.elapsed().as_secs_f64() * 1e3 / zs.len() as f64;
    let found = depth_from_focus(&stack, &zs, ((0.2 / p).round() as usize).max(1), 9);
    let mut errors: Vec<f64> = found
        .iter()
        .zip(&truth)
        .filter(|(_, t)| !t.is_nan())
        .map(|(f, t)| f64::from((f - t).abs()))
        .collect();
    errors.sort_by(f64::total_cmp);
    let median = errors[errors.len() / 2];
    let within = errors.iter().filter(|e| **e < 0.25).count() as f64 / errors.len() as f64;
    std::fs::write(
        page.join("surface-truth.png"),
        png(&heights_image(&truth, lo, hi, w, h), 1.0)?,
    )?;
    std::fs::write(
        page.join("surface-found.png"),
        png(&heights_image(&found, lo, hi, w, h), 1.0)?,
    )?;
    std::fs::write(page.join("surface-height.f32"), raw(&truth))?;
    println!(
        "the surface at 20×, {w} × {h} at {p:.3} µm, over the die at ({:.0}, {:.0}) µm: {} planes, {ms:.0} ms each",
        centre.x,
        centre.y,
        zs.len()
    );
    println!(
        "  heights {lo:.2} to {hi:.2} µm; depth from focus: median error {median:.3} µm, {:.0}% of pixels within 0.25 µm",
        100.0 * within
    );
    Ok(json!({
        "objective": "20x", "na": optics.na, "pixel": p, "field": [w, h], "centre": [centre.x, centre.y],
        "planes": planes, "ms": ms, "range": [lo, hi], "truth": "surface-truth.png", "found": "surface-found.png",
        "median_error": median, "within": within,
        "raw": {"file": "surface-height.f32", "shape": [h, w], "type": "f32 little-endian, µm above the slide, NaN off the chip"},
    }))
}

/// The cells: a z-series through a simulated 60× oil objective, in
/// widefield and through a confocal, each the three lines' images added,
/// with slices of the true volume.
fn cells(page: &Path, seed: u64, smoke: bool) -> anyhow::Result<Value> {
    let culture = CellCulture::new(seed, CULTURE);
    let middle = Point2::new(CULTURE.origin.x + 5_000.0, CULTURE.origin.y + 5_000.0);
    let near = Rect::new(middle.x - 50.0, middle.y - 50.0, 100.0, 100.0);
    let cell = culture
        .cells(near)
        .into_iter()
        .next()
        .context("a cell near the culture's middle")?;
    let centre = cell.centre;
    let world = World::with_slide(
        Slide::new(Arc::new(culture.clone()), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    // An oil objective of NA 1.4, as confocal microscopes use, on 2 × 2
    // binned pixels; four times binned for --smoke.
    let optics = Optics {
        na: 1.4,
        magnification: 60.0,
        sensor_pixel: Um(if smoke { 9.6 } else { 4.8 }),
        wavelength: Um(0.55),
        medium: 1.518,
    };
    let p = optics.pixel_size().0;
    let (w, h) = if smoke { (160, 120) } else { (640, 480) };
    let zs: Vec<f64> = if smoke {
        vec![0.0, 1.5, 3.0, 4.5, 6.0]
    } else {
        (0..=12).map(|i| f64::from(i) * 0.5).collect()
    };
    let composite = |z: f64, confocal: bool| {
        let field = Field {
            centre,
            z: Um(FOCUS + z),
            width: w,
            height: h,
        };
        let mut sum = Image::zeros(Shape::new(w, h, 3));
        for line in LINES {
            let image = world.render(&field, &optics, Light::Fluorescence { line, confocal });
            for (s, v) in sum.data_mut().iter_mut().zip(image.data()) {
                *s += v;
            }
        }
        sum
    };
    let started = Instant::now();
    let series: Vec<(Image, Image)> = zs
        .iter()
        .map(|z| (composite(*z, false), composite(*z, true)))
        .collect();
    let ms = started.elapsed().as_secs_f64() * 1e3 / (2 * zs.len()) as f64;
    // One white for every image: the widefield's brightest plane, near its
    // top, so the confocal's dimming shows.
    let mut values: Vec<f32> = series
        .iter()
        .flat_map(|(wide, _)| wide.data().to_vec())
        .collect();
    let at = values.len() * 998 / 1000;
    let white = values
        .select_nth_unstable_by(at, f32::total_cmp)
        .1
        .max(1e-6);

    let origin = Point2::new(
        centre.x - f64::from(w) * p / 2.0,
        centre.y - f64::from(h) * p / 2.0,
    );
    let colours = [[0.25, 0.45, 1.0], [0.2, 1.0, 0.3], [1.0, 0.35, 0.1]];
    let mut truth_raw = Vec::new();
    let mut planes = Vec::new();
    for (i, (z, (wide, conf))) in zs.iter().zip(&series).enumerate() {
        let truth = culture.truth(origin, Um(p), w, h, Um(*z));
        truth_raw.extend_from_slice(truth.data());
        let shown: Vec<f32> = truth
            .data()
            .chunks(3)
            .flat_map(|d| {
                let mut rgb = [0.0_f32; 3];
                for ((density, full), colour) in d.iter().zip(DENSITY).zip(colours) {
                    let share = density / full as f32;
                    for (out, c) in rgb.iter_mut().zip(colour) {
                        *out += 0.8 * share * c;
                    }
                }
                rgb
            })
            .collect();
        let shown = Image::from_vec(Shape::new(w, h, 3), shown)?;
        let energy = |image: &Image| {
            image.data().iter().map(|v| f64::from(*v)).sum::<f64>() / f64::from(w * h)
        };
        let files = [
            format!("cells-wide-{i}.png"),
            format!("cells-confocal-{i}.png"),
            format!("cells-truth-{i}.png"),
        ];
        std::fs::write(page.join(&files[0]), png(wide, white)?)?;
        std::fs::write(page.join(&files[1]), png(conf, white)?)?;
        std::fs::write(page.join(&files[2]), png(&shown, 1.0)?)?;
        planes.push(json!({
            "z": z, "widefield": files[0], "confocal": files[1], "truth": files[2],
            "widefield_light": energy(wide), "confocal_light": energy(conf),
        }));
    }
    std::fs::write(page.join("cells-truth.f32"), raw(&truth_raw))?;
    let section = optical_section(&optics, DYES[1].peaks().1).0;
    let dry = {
        let catalogue = ObjectiveCatalogue::mitutoyo();
        optical_section(
            &Optics::new(&catalogue, catalogue.get("50x")?, Um(2.4)),
            DYES[1].peaks().1,
        )
        .0
    };
    println!(
        "the cells through a 60× oil objective of NA 1.4, {w} × {h} at {p:.3} µm, on the cell at ({:.0}, {:.0}) µm: {} planes, {ms:.0} ms an image of three lines",
        centre.x,
        centre.y,
        zs.len()
    );
    println!(
        "  the confocal's optical section: {section:.2} µm here; {dry:.2} µm through the instrument's dry 50×"
    );
    println!("  {:>6} {:>10} {:>10}", "z", "widefield", "confocal");
    for plane in &planes {
        println!(
            "  {:>3.1} µm {:>10.4} {:>10.4}",
            plane["z"].as_f64().unwrap_or(0.0),
            plane["widefield_light"].as_f64().unwrap_or(0.0),
            plane["confocal_light"].as_f64().unwrap_or(0.0)
        );
    }
    Ok(json!({
        "objective": "60x oil, NA 1.4 (simulated)", "pixel": p, "field": [w, h], "centre": [centre.x, centre.y],
        "section": section, "dry_section": dry, "planes": planes, "ms": ms, "cells_in_view": culture.cells(Rect::new(origin.x, origin.y, f64::from(w) * p, f64::from(h) * p)).len(),
        "raw": {"file": "cells-truth.f32", "shape": [zs.len(), h, w, 3], "z": zs, "dyes": ["DAPI: nuclei", "green: membranes", "orange: organelles"],
                "type": "f32 little-endian, each dye's density"},
    }))
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-sim-3d", 8216, &["--seed"])?;
    let seed = example.number("--seed").unwrap_or(1) as u64;
    let page = example.roots.data.join("page");
    std::fs::create_dir_all(&page)?;
    for file in ["index.html", "app.js"] {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("page")
                .join(file),
            page.join(file),
        )?;
    }
    let surface = surface(&page, seed, example.smoke)?;
    let cells = cells(&page, seed, example.smoke)?;

    // A plane rendered twice must be the same, to the bit.
    let culture = CellCulture::new(seed, CULTURE);
    let world = World::with_slide(
        Slide::new(Arc::new(culture), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("50x")?, Um(2.4));
    let field = Field {
        centre: Point2::new(45_000.0, 10_000.0),
        z: Um(FOCUS + 2.0),
        width: 96,
        height: 64,
    };
    let light = Light::Fluorescence {
        line: Excitation::Blue,
        confocal: true,
    };
    let repeatable =
        world.render(&field, &optics, light).data() == world.render(&field, &optics, light).data();

    let results =
        json!({"seed": seed, "surface": surface, "cells": cells, "repeatable": repeatable});
    std::fs::write(
        page.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let api = Api::builder(&example.roots);
    example.serve(api, &page, smoke).await
}

/// `--smoke`: the page and its files are served; depth from focus finds
/// the surface's heights; the confocal cuts the light from above the cells
/// that widefield keeps; and a plane renders the same twice.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/results.json", "application/json"),
        ("/surface-0.png", "image/png"),
        ("/surface-found.png", "image/png"),
        ("/cells-confocal-2.png", "image/png"),
        ("/cells-truth.f32", ""),
    ] {
        let file = get(address, path).await?;
        ensure!(
            file.status == 200 && file.content_type.starts_with(kind),
            "{path}"
        );
    }
    let results: Value = get(address, "/results.json").await?.json()?;
    ensure!(
        results["repeatable"] == true,
        "a plane rendered differently twice"
    );
    let median = results["surface"]["median_error"]
        .as_f64()
        .context("the median error")?;
    ensure!(
        median < 0.25,
        "depth from focus: a median error of {median} µm"
    );
    let planes = results["cells"]["planes"]
        .as_array()
        .context("the cells' planes")?;
    let light = |i: usize, mode: &str| planes[i][mode].as_f64().unwrap_or(0.0);
    // From the cells' middle, at 3 µm, to 6 µm, above most of them.
    let (mid, top) = (2, planes.len() - 1);
    let wide = light(top, "widefield_light") / light(mid, "widefield_light");
    let conf = light(top, "confocal_light") / light(mid, "confocal_light");
    ensure!(
        wide > 0.7 && conf < 0.8 * wide,
        "above the cells: widefield keeps {wide:.2}, the confocal {conf:.2}"
    );
    println!(
        "served: the page, the surface's planes and maps, the cells' planes and truth; depth from focus within {median:.3} µm; above the cells the confocal keeps {conf:.2} of the light, widefield {wide:.2}"
    );
    Ok(())
}
