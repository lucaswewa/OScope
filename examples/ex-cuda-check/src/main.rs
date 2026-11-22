//! `ex-cuda-check`: the GPU that CUDA uses, or why there is none; each CUDA
//! kernel against its CPU twin, with their times; and how fast a 40 MB frame
//! goes to the GPU from pinned and from ordinary memory (P07). See the
//! README.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::Context;
use oscope_compute::{Backend, Compute, ComputeConfig};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-cuda-check: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The median of `runs` timings of `work`.
#[cfg_attr(not(feature = "cuda"), allow(dead_code))]
fn median(runs: usize, mut work: impl FnMut() -> anyhow::Result<()>) -> anyhow::Result<Duration> {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        work()?;
        times.push(start.elapsed());
    }
    times.sort();
    Ok(times[runs / 2])
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut smoke, mut runs) = (false, 10);
    let (mut require_cuda, mut device) = (false, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke" => smoke = true,
            "--require-cuda" => require_cuda = true,
            "--device" => device = Some(args.next().context("--device needs a number")?.parse()?),
            "--runs" => runs = args.next().context("--runs needs a number")?.parse()?,
            other => anyhow::bail!(
                "`{other}`: the options are --runs N, --smoke, --require-cuda and --device N"
            ),
        }
    }
    let runs = if smoke { 3 } else { runs.max(1) };
    let compute = Compute::new(&ComputeConfig {
        cuda_device: device,
        ..ComputeConfig::default()
    })?;
    anyhow::ensure!(
        !require_cuda || compute.info().backend() == Backend::Cuda,
        "CUDA required but unavailable: {:?}",
        compute.info().backends
    );
    println!("backends, the one in use first:");
    for backend in &compute.info().backends {
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
    #[cfg(feature = "cuda")]
    if let Some(cuda) = compute.cuda() {
        return gpu::check(&compute, cuda, runs, smoke);
    }
    let _ = runs;
    println!("\nno GPU in use, so OScope computes on the CPU");
    if smoke {
        println!("smoke: ok");
    }
    Ok(())
}

#[cfg(feature = "cuda")]
mod gpu {
    use anyhow::ensure;
    use oscope_compute::cuda::Cuda;
    use std::time::Duration;

    use oscope_compute::equivalence::{Comparison, Tolerance, compare_images};
    use oscope_compute::{Compute, Lane, cpu};
    use oscope_core::{BayerPattern, FramePool, FrameView, PixelFormat};

    use super::median;

    /// The MIChrome 20's full frame.
    const WIDTH: u32 = 5472;
    const HEIGHT: u32 = 3648;

    /// The end of a timed piece of work, whose result isn't needed.
    fn done<T>(_result: T) -> anyhow::Result<()> {
        Ok(())
    }

    fn ms(time: Duration) -> String {
        format!("{:>7.2} ms", time.as_secs_f64() * 1e3)
    }

    /// Prints a kernel's times on the CPU, on the GPU, and from the host and
    /// back, and how its output compares; returns whether it matched.
    fn row(name: &str, [cpu, gpu, trip]: [Duration; 3], comparison: Comparison) -> bool {
        let verdict = match comparison.passed() {
            true => "exact".to_owned(),
            false => comparison.to_string(),
        };
        let speed = cpu.as_secs_f64() / gpu.as_secs_f64();
        println!(
            "  {name:<14} {} {} {}    {speed:>5.1} ×  {verdict}",
            ms(cpu),
            ms(gpu),
            ms(trip)
        );
        comparison.passed()
    }

    pub fn check(compute: &Compute, cuda: &Cuda, runs: usize, smoke: bool) -> anyhow::Result<()> {
        let gpu = cuda.info();
        println!(
            "\nGPU {}: {}, compute {}.{}, {} MB; the driver supports CUDA {}",
            gpu.ordinal,
            gpu.name,
            gpu.compute_capability.0,
            gpu.compute_capability.1,
            gpu.memory_bytes >> 20,
            gpu.driver_cuda
        );
        println!(
            "memory pool kept: {}; NVRTC: {}; cuFFT: {}",
            gpu.memory_pool, gpu.nvrtc, gpu.cufft
        );

        // A 20 MP frame in the frame pool's pinned memory, as the camera
        // will deliver it.
        let format = PixelFormat::Bayer16 {
            pattern: BayerPattern::Rggb,
            significant_bits: 16,
        };
        let len = format.frame_bytes(WIDTH, HEIGHT);
        let pool = FramePool::new(len, 1, &cuda.pinned_allocator());
        let mut buffer = pool.take().expect("one slab");
        for (i, pair) in buffer
            .bytes_mut()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            *pair = ((i * 13) as u16).to_le_bytes();
        }
        let view = FrameView::new(buffer.bytes(), format, WIDTH, HEIGHT)?;
        let image = cpu::to_f32(&view)?;
        let lane = Lane::Live;

        println!(
            "\nthe median of {runs} runs on a {WIDTH} × {HEIGHT} RGGB16 frame, in the live lane:"
        );
        println!(
            "  {:<14} {:>10} {:>10} {:>10} {:>9}  against the CPU",
            "", "CPU", "GPU", "round trip", "speed-up"
        );
        let on_gpu = cuda.to_f32(lane, &view)?;
        let mut all_exact = true;

        // to_f32 starts from the host, so its GPU time includes the upload.
        let cpu_time = median(runs, || done(compute.to_f32(lane, &view)?))?;
        let gpu_time = median(runs, || {
            cuda.to_f32(lane, &view)?;
            Ok(cuda.synchronize(lane)?)
        })?;
        let trip = median(runs, || {
            done(cuda.download(lane, &cuda.to_f32(lane, &view)?)?)
        })?;
        let check = compare_images(&image, &cuda.download(lane, &on_gpu)?, Tolerance::EXACT);
        all_exact &= row("to_f32", [cpu_time, gpu_time, trip], check?);

        let cpu_time = median(runs, || done(compute.bin2x2(lane, &image)))?;
        let gpu_time = median(runs, || {
            cuda.bin2x2(lane, &on_gpu)?;
            Ok(cuda.synchronize(lane)?)
        })?;
        let trip = median(runs, || {
            let up = cuda.upload(lane, &image)?;
            done(cuda.download(lane, &cuda.bin2x2(lane, &up)?)?)
        })?;
        let binned = cuda.download(lane, &cuda.bin2x2(lane, &on_gpu)?)?;
        let check = compare_images(&cpu::bin2x2(&image), &binned, Tolerance::EXACT);
        all_exact &= row("bin2x2", [cpu_time, gpu_time, trip], check?);

        let cpu_time = median(runs, || done(compute.downsample(lane, &image, 4)?))?;
        let gpu_time = median(runs, || {
            cuda.downsample(lane, &on_gpu, 4)?;
            Ok(cuda.synchronize(lane)?)
        })?;
        let trip = median(runs, || {
            let up = cuda.upload(lane, &image)?;
            done(cuda.download(lane, &cuda.downsample(lane, &up, 4)?)?)
        })?;
        let small = cuda.download(lane, &cuda.downsample(lane, &on_gpu, 4)?)?;
        let check = compare_images(&cpu::downsample(&image, 4)?, &small, Tolerance::EXACT);
        all_exact &= row("downsample ×4", [cpu_time, gpu_time, trip], check?);

        // The same 40 MB to the GPU, from the pool's pinned slab and from an
        // ordinary vector.
        let ordinary = buffer.bytes().to_vec();
        let pinned = cuda.is_pinned(buffer.bytes());
        println!("\nuploading the 40 MB frame:");
        for (source, bytes) in [("pinned", buffer.bytes()), ("ordinary", &ordinary[..])] {
            let time = median(runs, || done(cuda.upload_bytes(lane, bytes)?))?;
            let rate = len as f64 / time.as_secs_f64() / 1e9;
            println!("  from {source:<9} memory {}  {rate:>5.1} GB/s", ms(time));
        }

        if smoke {
            ensure!(all_exact, "a CUDA kernel differs from its CPU twin");
            ensure!(
                pinned && !cuda.is_pinned(&ordinary),
                "the pool's memory isn't pinned"
            );
            println!("smoke: ok");
        }
        Ok(())
    }
}
