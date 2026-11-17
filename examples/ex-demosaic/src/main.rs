//! `ex-demosaic`: each demosaicing method on a 20 MP test chart of known
//! colours, mosaicked as the MIChrome 20 sees it.
//!
//! - **Its time** on the CPU (P08) and, with the `cuda` feature, on the
//!   GPU (P08c), against §5.20's budgets.
//! - **Its PSNR.**
//! - **Its images,** as PNG and 16-bit TIFF (P08b), on a page that compares
//!   the methods side by side (P08c).
//!
//! See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_compute::{Compute, ComputeConfig, Image, Lane, cpu};
use oscope_core::{
    BayerPattern, FramePool, FrameView, HeapAllocator, PixelFormat, PixelRect, SlabAllocator,
};
use oscope_imaging::color::{
    Demosaic, Display, chart, demosaic_frame, demosaic_region, encode_png, encode_tiff16, mosaic,
    psnr, to_display,
};
use serde_json::{Value, json};

/// The MIChrome 20's full frame.
const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;

/// §5.20's budgets for a 20 MP demosaic: on 8 CPU threads, and with CUDA.
const CPU_BUDGET_MS: f64 = 80.0;
const GPU_BUDGET_MS: f64 = 10.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-demosaic: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The median of `runs` timings of `work`, in milliseconds.
fn median(runs: usize, mut work: impl FnMut() -> anyhow::Result<()>) -> anyhow::Result<f64> {
    let mut times: Vec<Duration> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        work()?;
        times.push(start.elapsed());
    }
    times.sort();
    Ok(times[runs / 2].as_secs_f64() * 1e3)
}

/// An image through the display transform, as a PNG file's bytes.
fn png(rgb: &Image) -> anyhow::Result<Vec<u8>> {
    let shown = to_display(rgb, &Display::default())?;
    let mut file = Vec::new();
    encode_png(&mut file, &shown, rgb.width(), rgb.height())?;
    Ok(file)
}

/// Pinned memory for the frame if there is a GPU, as the camera will
/// deliver frames; ordinary memory otherwise.
fn allocator(compute: &Compute) -> Box<dyn SlabAllocator> {
    #[cfg(feature = "cuda")]
    if let Some(cuda) = compute.cuda() {
        return Box::new(cuda.pinned_allocator());
    }
    let _ = compute;
    Box::new(HeapAllocator)
}

/// The GPU's time for `method` from the raw frame, and whether its result
/// equals the CPU's to the bit; nothing without a GPU.
#[cfg(feature = "cuda")]
fn on_gpu(
    compute: &Compute,
    view: &FrameView<'_>,
    method: Demosaic,
    cpu: &Image,
    runs: usize,
) -> anyhow::Result<(Option<f64>, Option<bool>)> {
    use oscope_compute::equivalence::{Tolerance, compare_images};
    use oscope_imaging::color::gpu;
    let Some(cuda) = compute.cuda() else {
        return Ok((None, None));
    };
    let time = median(runs, || {
        gpu::demosaic_frame(cuda, Lane::Live, view, method)?;
        Ok(cuda.synchronize(Lane::Live)?)
    })?;
    let rgb = cuda.download(
        Lane::Live,
        &gpu::demosaic_frame(cuda, Lane::Live, view, method)?,
    )?;
    let same = compare_images(cpu, &rgb, Tolerance::EXACT)?.passed();
    Ok((Some(time), Some(same)))
}

#[cfg(not(feature = "cuda"))]
fn on_gpu(
    _: &Compute,
    _: &FrameView<'_>,
    _: Demosaic,
    _: &Image,
    _: usize,
) -> anyhow::Result<(Option<f64>, Option<bool>)> {
    Ok((None, None))
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-demosaic", 8214, &["--runs", "--threads"])?;
    let runs = example.number("--runs");
    let runs = runs.unwrap_or(if example.smoke { 3 } else { 10 }).max(1);
    let threads = example.number("--threads").unwrap_or(8).max(1);
    let config = ComputeConfig {
        live_threads: Some(threads),
        ..ComputeConfig::default()
    };
    let compute = Compute::new(&config)?;
    #[cfg(feature = "cuda")]
    let gpu_name = compute.cuda().map(|cuda| cuda.info().name.clone());
    #[cfg(not(feature = "cuda"))]
    let gpu_name: Option<String> = None;

    // The chart, as a 16-bit RGGB frame from the frame pool.
    let truth = chart(WIDTH, HEIGHT);
    let raw = mosaic(&truth, BayerPattern::Rggb)?;
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let pool = FramePool::new(
        format.frame_bytes(WIDTH, HEIGHT),
        1,
        allocator(&compute).as_ref(),
    );
    let mut buffer = pool.take().context("a slab for the frame")?;
    for (sample, v) in buffer
        .bytes_mut()
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(raw.data())
    {
        *sample = ((v * 65_535.0).round() as u16).to_le_bytes();
    }
    let view = FrameView::new(buffer.bytes(), format, WIDTH, HEIGHT)?;
    let half = cpu::downsample(&truth, 2)?;
    let floats = compute.to_f32(Lane::Live, &view)?;

    // The page's folder: its files, then the images and results beside them.
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

    println!(
        "a {WIDTH} × {HEIGHT} chart as RGGB16; the median of {runs} runs, from the raw frame to RGB:"
    );
    println!(
        "  {:<16} {:>12} {:>12} {:>8}  GPU against CPU",
        "method", "CPU", "GPU", "PSNR"
    );
    let mut methods = Vec::new();
    for (method, mark) in Demosaic::ALL.into_iter().zip([21.0, 20.5, 29.0]) {
        let mut rgb = None;
        let cpu_ms = median(runs, || {
            rgb = Some(demosaic_frame(&compute, Lane::Live, &view, method)?);
            Ok(())
        })?;
        let rgb = rgb.context("at least one run")?;
        // Superpixel's image is half size: it is measured against the chart
        // averaged over 2 × 2 cells.
        let (reference, border, slug, name, scale) = match method {
            Demosaic::Superpixel => (&half, 2, "superpixel", "superpixel", 2),
            Demosaic::Bilinear => (&truth, 4, "bilinear", "bilinear", 1),
            Demosaic::MalvarHeCutler => (&truth, 4, "malvar-he-cutler", "Malvar–He–Cutler", 1),
        };
        let quality = psnr(reference, &rgb, border)?;
        let (gpu_ms, same) = on_gpu(&compute, &view, method, &rgb, runs)?;

        // The fine rings at full resolution, in the method's own pixels.
        let rings = PixelRect::new(4304 / scale, 2480 / scale, 512 / scale, 512 / scale);
        let region = demosaic_region(&floats, BayerPattern::Rggb, method, rings, 1)?;
        let mut tiff = Vec::new();
        encode_tiff16(&mut tiff, &region)?;
        std::fs::write(page.join(format!("{slug}-rings.png")), png(&region)?)?;
        std::fs::write(page.join(format!("{slug}-rings.tif")), tiff)?;
        std::fs::write(
            page.join(format!("{slug}.png")),
            png(&cpu::downsample(&rgb, 4)?)?,
        )?;

        let gpu_text = gpu_ms.map_or("—".to_owned(), |t| format!("{t:.1} ms"));
        let same_text = same.map_or("—", |same| if same { "to the bit" } else { "differs" });
        println!("  {name:<16} {cpu_ms:>9.1} ms {gpu_text:>12} {quality:>5.1} dB  {same_text}");
        // What `cargo xtask perf` reads (P53).
        println!("perf: demosaic.{slug}.cpu_ms {cpu_ms:.2}");
        if let Some(gpu_ms) = gpu_ms {
            println!("perf: demosaic.{slug}.gpu_ms {gpu_ms:.2}");
        }
        methods.push(json!({
            "name": name, "slug": slug, "cpu_ms": cpu_ms, "gpu_ms": gpu_ms, "matches_cpu": same,
            "psnr": quality, "mark": mark, "width": rgb.width(), "height": rgb.height(),
        }));
    }
    let results = json!({
        "chart": [WIDTH, HEIGHT], "threads": threads, "runs": runs, "gpu": gpu_name,
        "budgets": {"cpu_ms": CPU_BUDGET_MS, "gpu_ms": GPU_BUDGET_MS}, "methods": methods,
    });
    std::fs::write(
        page.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;

    let api = Api::builder(&example.roots);
    example.serve(api, &page, smoke).await
}

/// `--smoke`: the page, its images and its results are served; each method
/// meets its PSNR mark, and each CUDA twin, if any ran, matched the CPU.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let files = [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/results.json", "application/json"),
        ("/malvar-he-cutler-rings.png", "image/png"),
        ("/bilinear-rings.tif", "image/tiff"),
    ];
    for (path, kind) in files {
        let file = get(address, path).await?;
        ensure!(
            file.status == 200 && file.content_type.starts_with(kind),
            "{path}"
        );
    }
    let results: Value = get(address, "/results.json").await?.json()?;
    let methods = results["methods"].as_array().context("the methods")?;
    ensure!(methods.len() == 3, "{} methods", methods.len());
    for method in methods {
        let (psnr, mark) = (method["psnr"].as_f64(), method["mark"].as_f64());
        ensure!(psnr > mark, "{} fell below its PSNR mark", method["name"]);
        ensure!(
            method["matches_cpu"] != false,
            "{}'s CUDA twin differs",
            method["name"]
        );
    }
    println!("served: the page, its images and its results");
    Ok(())
}
