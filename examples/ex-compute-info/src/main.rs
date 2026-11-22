//! `ex-compute-info`: the compute layer's backends, the processor and its
//! pools, and microbenchmarks of the first kernels on a 20 MP frame (P06).
//! See the README.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use oscope_compute::equivalence::{Tolerance, compare_images};
use oscope_compute::{Compute, ComputeConfig, Lane, Shape};
use oscope_core::{BayerPattern, FrameView, PixelFormat};

/// The MIChrome 20's full frame.
const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-compute-info: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// A kernel to time, run in a lane.
type Kernel<'a> = &'a dyn Fn(Lane) -> anyhow::Result<()>;

/// The median of `runs` timings of `kernel`.
fn median(runs: usize, mut kernel: impl FnMut() -> anyhow::Result<()>) -> anyhow::Result<Duration> {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        kernel()?;
        times.push(start.elapsed());
    }
    times.sort();
    Ok(times[runs / 2])
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut smoke, mut runs) = (false, 10);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke" => smoke = true,
            "--runs" => runs = args.next().context("--runs needs a number")?.parse()?,
            other => anyhow::bail!("`{other}`: the options are --runs N and --smoke"),
        }
    }
    let runs = if smoke { 3 } else { runs.max(1) };

    let compute = Compute::new(&ComputeConfig::default())?;
    let info = compute.info();
    println!("backends:");
    for backend in &info.backends {
        let state = if backend.available {
            "available"
        } else {
            "not available"
        };
        println!(
            "  {:<5} {state:<14} {}",
            format!("{:?}", backend.backend),
            backend.detail
        );
    }
    println!(
        "processor: {} threads, {:?}; pools: {} live, {} batch",
        info.cpu_threads, info.simd, info.live_threads, info.batch_threads
    );

    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let bytes: Vec<u8> = (0..WIDTH * HEIGHT)
        .flat_map(|i| ((i % 65_536) as u16).to_le_bytes())
        .collect();
    let view = FrameView::new(&bytes, format, WIDTH, HEIGHT)?;
    let image = compute.to_f32(Lane::Live, &view)?;
    let floats = image.data().len() * 4;

    println!("\nthe median of {runs} runs on a {WIDTH} × {HEIGHT} RGGB16 frame:");
    println!("  {:<14} {:>22} {:>22}", "", "live", "batch");
    let kernels: [(&str, usize, Kernel); 3] = [
        ("to_f32", bytes.len(), &|lane| {
            compute.to_f32(lane, &view)?;
            Ok(())
        }),
        ("bin2x2", floats, &|lane| {
            compute.bin2x2(lane, &image);
            Ok(())
        }),
        ("downsample ×4", floats, &|lane| {
            compute.downsample(lane, &image, 4)?;
            Ok(())
        }),
    ];
    for (name, input, kernel) in kernels {
        print!("  {name:<14}");
        for lane in [Lane::Live, Lane::Batch] {
            let time = median(runs, || kernel(lane))?;
            let rate = input as f64 / time.as_secs_f64() / 1e9;
            print!(" {:>9.2} ms {:>6.2} GB/s", time.as_secs_f64() * 1e3, rate);
        }
        println!();
    }

    // Binning sums the blocks that downsampling by 2 averages.
    let binned = compute.bin2x2(Lane::Live, &image);
    let mut quartered = compute.downsample(Lane::Live, &image, 2)?;
    quartered.data_mut().iter_mut().for_each(|v| *v *= 4.0);
    let comparison = compare_images(&binned, &quartered, Tolerance::abs(1e-6))?;
    println!("\nbin2x2 against 4 × downsample ×2: {comparison}");

    if smoke {
        ensure!(info.backends[0].available && info.live_threads >= 1 && info.batch_threads >= 1);
        let last = f32::from(((WIDTH * HEIGHT - 1) % 65_536) as u16) / 65_535.0;
        ensure!(
            image.get(WIDTH - 1, HEIGHT - 1, 0) == Some(last),
            "to_f32's last sample"
        );
        ensure!(
            binned.shape() == Shape::new(2736, 1824, 1),
            "bin2x2's shape"
        );
        let preview = compute.downsample(Lane::Batch, &image, 4)?;
        ensure!(
            preview.shape() == Shape::new(1368, 912, 1),
            "downsample's shape"
        );
        ensure!(comparison.passed(), "the two kernels disagree");
        println!("smoke: ok");
    }
    Ok(())
}
