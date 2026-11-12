//! `ex-demosaic`: each demosaicing method on a 20 MP test chart of known
//! colours, mosaicked as the MIChrome 20 sees it: its time, against §5.20's
//! 80 ms on 8 threads, and its PSNR (P08). See the README.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use oscope_compute::{Compute, ComputeConfig, Lane, cpu};
use oscope_core::{BayerPattern, FrameView, PixelFormat};
use oscope_imaging::color::{Demosaic, chart, demosaic_frame, mosaic, psnr};

/// The MIChrome 20's full frame.
const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;

/// §5.20's budget for a 20 MP demosaic on 8 CPU threads.
const BUDGET: Duration = Duration::from_millis(80);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-demosaic: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut smoke, mut runs, mut threads) = (false, 10, 8);
    while let Some(arg) = args.next() {
        let mut number = |name: &str| -> anyhow::Result<usize> {
            Ok(args
                .next()
                .with_context(|| format!("{name} needs a number"))?
                .parse()?)
        };
        match arg.as_str() {
            "--smoke" => smoke = true,
            "--runs" => runs = number("--runs")?.max(1),
            "--threads" => threads = number("--threads")?.max(1),
            other => anyhow::bail!("`{other}`: the options are --runs N, --threads N and --smoke"),
        }
    }
    let runs = if smoke { 3 } else { runs };
    let config = ComputeConfig {
        live_threads: Some(threads),
        ..ComputeConfig::default()
    };
    let compute = Compute::new(&config)?;

    // The chart, as a 16-bit RGGB frame.
    let truth = chart(WIDTH, HEIGHT);
    let raw = mosaic(&truth, BayerPattern::Rggb)?;
    let bytes: Vec<u8> = raw
        .data()
        .iter()
        .flat_map(|v| ((v * 65_535.0).round() as u16).to_le_bytes())
        .collect();
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let view = FrameView::new(&bytes, format, WIDTH, HEIGHT)?;
    let half = cpu::downsample(&truth, 2)?;

    println!(
        "a {WIDTH} × {HEIGHT} test chart, mosaicked as RGGB16, demosaiced in a live pool of {threads} threads;"
    );
    println!("the median of {runs} runs, from the raw frame to RGB:\n");
    println!(
        "  {:<16} {:>9}  {:<12} PSNR",
        "method", "time", "within 80 ms"
    );
    let marks = [21.0, 20.5, 29.0];
    let mut passed = true;
    for (method, mark) in Demosaic::ALL.into_iter().zip(marks) {
        let mut times = Vec::with_capacity(runs);
        let mut rgb = None;
        for _ in 0..runs {
            let start = Instant::now();
            rgb = Some(demosaic_frame(&compute, Lane::Live, &view, method)?);
            times.push(start.elapsed());
        }
        times.sort();
        let time = times[runs / 2];
        let rgb = rgb.expect("at least one run");
        // Superpixel's output is half size: it is measured against the chart
        // averaged over 2 × 2 cells.
        let (reference, border) = match method {
            Demosaic::Superpixel => (&half, 2),
            _ => (&truth, 4),
        };
        let quality = psnr(reference, &rgb, border)?;
        passed &= quality > mark;
        let within = if time <= BUDGET { "yes" } else { "no" };
        let name = match method {
            Demosaic::Superpixel => "superpixel",
            Demosaic::Bilinear => "bilinear",
            Demosaic::MalvarHeCutler => "Malvar–He–Cutler",
        };
        let ms = time.as_secs_f64() * 1e3;
        println!(
            "  {name:<16} {ms:>6.1} ms  {within:<12} {quality:.1} dB, {}×{}",
            rgb.width(),
            rgb.height()
        );
    }

    if smoke {
        ensure!(passed, "a method fell below its PSNR mark");
        println!("smoke: ok");
    }
    Ok(())
}
