//! `ex-sim-render`: the simulated H&E slide, seen through each objective at
//! several distances from focus. It renders an overview of the slide, then
//! fields at 5×, 10×, 20× and 50×, each at five focus positions a depth of
//! field apart, and records each field's rendering time and sharpness. It
//! writes them as PNGs, and serves a page that shows them. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::{Display, encode_png, to_display};
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};
use serde_json::{Value, json};

/// Where the slide's corner sits on the stage, and the stage z at which its
/// surface is in focus.
const SLIDE_AT: Point2 = Point2::new(10_000.0, 20_000.0);
const FOCUS: Um = Um(5_000.0);
/// The height of the section's middle above the slide.
const SECTION: f64 = 2.0;
/// The focus positions, in depths of field from the section's middle.
const STEPS: [f64; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-render: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// An image through the display transform, as a PNG file's bytes.
fn png(image: &Image) -> anyhow::Result<Vec<u8>> {
    let shown = to_display(image, &Display::default())?;
    let mut file = Vec::new();
    encode_png(&mut file, &shown, image.width(), image.height())?;
    Ok(file)
}

/// The mean step in green, the channel the stains darken most, between
/// pixels about 0.3 µm apart, near the finest detail the objectives resolve:
/// it falls as the image blurs.
fn sharpness(image: &Image, pixel: f64) -> f64 {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let k = ((0.3 / pixel).round() as usize).max(1);
    let g = |x: usize, y: usize| f64::from(image.data()[(y * w + x) * 3 + 1]);
    let mut sum = 0.0;
    for y in 0..h - k {
        for x in 0..w - k {
            sum += (g(x + k, y) - g(x, y)).abs() + (g(x, y + k) - g(x, y)).abs();
        }
    }
    sum / ((w - k) * (h - k)) as f64
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-sim-render", 8215, &["--seed", "--width"])?;
    let seed = example.number("--seed").unwrap_or(1) as u64;
    let width = example
        .number("--width")
        .unwrap_or(if example.smoke { 192 } else { 768 }) as u32;
    let height = width * 2 / 3;

    let tissue = HeTissue::new(seed, Slide::COVERSLIP);
    // The fields are centred on the nucleus nearest the first island's
    // centre, from the tissue's ground truth.
    let island = tissue.islands()[0];
    let near = Rect::new(island.x - 100.0, island.y - 100.0, 200.0, 200.0);
    let nucleus = tissue
        .nuclei(near)
        .into_iter()
        .min_by(|a, b| {
            a.centre
                .distance(island)
                .total_cmp(&b.centre.distance(island))
        })
        .context("a nucleus near the island's centre")?;
    let centre = Point2::new(SLIDE_AT.x + nucleus.centre.x, SLIDE_AT.y + nucleus.centre.y);
    let world = World::with_slide(Slide::new(Arc::new(tissue), SLIDE_AT), FOCUS);
    let catalogue = ObjectiveCatalogue::mitutoyo();

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

    // The whole slide, at 25 µm a pixel, through a lens of NA 0.1.
    let (ow, oh) = if example.smoke {
        (300, 100)
    } else {
        (3000, 1000)
    };
    let scale = 75_000.0 / f64::from(ow);
    let overview_optics = Optics {
        na: 0.1,
        magnification: 2.4 / scale,
        sensor_pixel: Um(2.4),
        wavelength: Um(0.55),
        medium: 1.0,
    };
    let slide_centre = Point2::new(SLIDE_AT.x + 37_500.0, SLIDE_AT.y + 12_500.0);
    let started = Instant::now();
    let overview = world.render(
        &Field {
            centre: slide_centre,
            z: Um(FOCUS.0 + SECTION),
            width: ow,
            height: oh,
        },
        &overview_optics,
        Light::Transmitted,
    );
    std::fs::write(page.join("overview.png"), png(&overview)?)?;
    println!(
        "the slide, {ow} × {oh} at {scale:.0} µm a pixel: {:.0} ms",
        started.elapsed().as_secs_f64() * 1e3
    );

    println!(
        "fields of {width} × {height} at ({:.0}, {:.0}) µm on the stage, seed {seed}, on a nucleus in the section's {} half:",
        centre.x,
        centre.y,
        if nucleus.layer == 0 { "lower" } else { "upper" }
    );
    println!(
        "  {:<4} {:>8} {:>8} {:>9} {:>9} {:>9}",
        "", "pixel", "depth", "dz", "time", "sharpness"
    );
    let mut fields = Vec::new();
    for objective in &catalogue.objectives {
        let optics = Optics::new(&catalogue, objective, Um(2.4));
        let depth = optics.depth_of_field().0;
        for (i, step) in STEPS.iter().enumerate() {
            let dz = step * depth;
            let field = Field {
                centre,
                z: Um(FOCUS.0 + SECTION + dz),
                width,
                height,
            };
            let started = Instant::now();
            let image = world.render(&field, &optics, Light::Transmitted);
            let ms = started.elapsed().as_secs_f64() * 1e3;
            let file = format!("field-{}-{i}.png", objective.name);
            std::fs::write(page.join(&file), png(&image)?)?;
            let sharp = sharpness(&image, optics.pixel_size().0);
            println!(
                "  {:<4} {:>5.3} µm {:>5.1} µm {:>+6.2} µm {:>6.0} ms {:>9.4}",
                objective.name,
                optics.pixel_size().0,
                depth,
                dz,
                ms,
                sharp
            );
            fields.push(json!({
                "objective": objective.name, "na": objective.na, "pixel": optics.pixel_size().0,
                "depth": depth, "step": step, "dz": dz, "file": file, "ms": ms, "sharpness": sharp,
            }));
        }
    }
    // A second rendering of one field must be the same, to the bit.
    let optics = Optics::new(&catalogue, catalogue.get("20x")?, Um(2.4));
    let again = |world: &World| {
        world.render(
            &Field {
                centre,
                z: Um(FOCUS.0 + SECTION),
                width,
                height,
            },
            &optics,
            Light::Transmitted,
        )
    };
    let repeatable = again(&world).data() == again(&world).data();

    let results = json!({
        "seed": seed, "centre": [centre.x, centre.y], "slide": [SLIDE_AT.x, SLIDE_AT.y],
        "field": [width, height], "sensor_pixel": 2.4, "repeatable": repeatable,
        "nucleus_layer": nucleus.layer,
        "overview": {"file": "overview.png", "width": ow, "height": oh, "pixel": scale},
        "fields": fields,
    });
    std::fs::write(
        page.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;

    let api = Api::builder(&example.roots);
    example.serve(api, &page, smoke).await
}

/// `--smoke`: the page, its images and its results are served; each
/// objective is sharpest near focus; and a field renders the same twice.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/results.json", "application/json"),
        ("/overview.png", "image/png"),
        ("/field-20x-2.png", "image/png"),
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
        "a field rendered differently twice"
    );
    let fields = results["fields"].as_array().context("the fields")?;
    ensure!(fields.len() == 20, "{} fields", fields.len());
    for objective in fields.chunks(5) {
        let sharp: Vec<f64> = objective
            .iter()
            .map(|f| f["sharpness"].as_f64().unwrap_or(0.0))
            .collect();
        // The sharpest is within half a depth of field of the section's
        // middle, and the ends are softer than their neighbours: at 50×
        // that can be a half-layer either side of the middle.
        let best = (0..5)
            .max_by(|a, b| sharp[*a].total_cmp(&sharp[*b]))
            .unwrap_or(0);
        ensure!(
            (1..=3).contains(&best) && sharp[1] > sharp[0] && sharp[3] > sharp[4],
            "{}: sharpness {sharp:?} doesn't fall away from focus",
            objective[0]["objective"]
        );
    }
    println!("served: the page, the slide, 20 fields, sharpest near focus");
    Ok(())
}
