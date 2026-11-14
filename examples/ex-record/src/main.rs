//! `ex-record`: records the simulated camera's frames at the design rate,
//! 16 bits at 14.5 fps, to the scratch disk; says how fast it wrote them and
//! whether any were dropped; then ingests the recording into a project, as a
//! job. See the README.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use oscope_acquire::{Acquisition, FrameBus, INGEST, QUEUE, Recorder, RecorderStats, ingest_jobs};
use oscope_core::{
    BayerPattern, FramePool, HeapAllocator, Ms, PixelFormat, Point2, SystemClock, Um, Xyz,
};
use oscope_hal::fake::FakeCamera;
use oscope_hal::{Camera, CameraMode, ModeRate, ObjectiveCatalogue};
use oscope_jobs::{JobService, JobState, JobsConfig};
use oscope_sim::{
    HeTissue, Optics, Rate, SimCamera, SimCameraConfig, SimClock, SimStage, SimStageConfig, Slide,
    World,
};
use oscope_store::{
    ImageReader, NewProject, RecordingOptions, Samples, Scratch, ScratchPolicy, Store,
};
use serde_json::{Map, json};

/// The MIChrome 20's frames at the design rate: 16 bits, of which 12 carry
/// data.
const FORMAT: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 12,
};
const FOCUS: f64 = 1_000.0;

/// What the command line asks for.
struct Args {
    seconds: f64,
    keep_gb: Option<f64>,
    test_sensor: bool,
    scratch: Option<PathBuf>,
    smoke: bool,
}

fn args() -> anyhow::Result<Args> {
    let mut parsed = Args {
        seconds: 10.0,
        keep_gb: None,
        test_sensor: false,
        scratch: None,
        smoke: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--seconds" => parsed.seconds = value()?.parse()?,
            "--keep-gb" => parsed.keep_gb = Some(value()?.parse()?),
            "--scratch" => parsed.scratch = Some(value()?.into()),
            "--test-sensor" => parsed.test_sensor = true,
            "--smoke" => parsed.smoke = true,
            other => bail!(
                "`{other}`: the options are --seconds N, --keep-gb N, --scratch FOLDER, \
                 --test-sensor and --smoke"
            ),
        }
    }
    ensure!(parsed.seconds > 0.0, "--seconds: more than 0");
    Ok(parsed)
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-record: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The test sensor: the fake camera's stripes in frames of the MIChrome 20,
/// or `1 / divide` of them each way, at the design rate.
fn test_sensor(divide: u32) -> FakeCamera {
    let (width, height) = (5472 / divide, 3648 / divide);
    let mut capabilities = FakeCamera::default_capabilities();
    capabilities.model = "test sensor".to_owned();
    (capabilities.sensor_width, capabilities.sensor_height) = (width, height);
    capabilities.modes = vec![CameraMode {
        name: "design".to_owned(),
        width,
        height,
        binning: 1,
        rates: vec![ModeRate {
            format: FORMAT,
            fps: 14.5,
        }],
        line_time: Some(Ms(0.019)),
    }];
    FakeCamera::with_capabilities(capabilities, Arc::new(SystemClock::new()))
}

/// The simulated MIChrome 20 in its `design` mode over H&E tissue at 10×,
/// the stage still, rendering on the GPU if built with `cuda` and one is
/// there; and which renders.
async fn simulated() -> anyhow::Result<(SimCamera, &'static str)> {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let clock = Arc::new(SimClock::new(Rate::RealTime));
    let start = Xyz::new(Um(island.x), Um(island.y), Um(FOCUS));
    let stage = SimStageConfig {
        require_homing: false,
        start,
        ..SimStageConfig::default()
    };
    let stage = Arc::new(SimStage::new(stage, Arc::clone(&clock)));
    let slide = Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0));
    let world = Arc::new(RwLock::new(World::with_slide(slide, Um(FOCUS))));
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let config = SimCameraConfig::default();
    let optics = Optics::new(&catalogue, catalogue.get("10x")?, config.pixel_size);
    let camera = SimCamera::new(config, clock, stage, world, optics);
    #[allow(unused_mut)]
    let mut renders = "the CPU";
    #[cfg(feature = "cuda")]
    if let Ok(cuda) = oscope_compute::cuda::Cuda::open(0) {
        camera.use_gpu(cuda, 512).map_err(anyhow::Error::msg)?;
        renders = "the GPU";
    }
    let mut settings = camera.settings().await?;
    settings.mode = "design".to_owned();
    settings.exposure = Ms(1.0);
    camera.configure(settings).await?;
    Ok((camera, renders))
}

/// A pool of 24 of `camera`'s frames, about 1 GB of the MIChrome 20's, and
/// a frame's bytes.
async fn pool(camera: &dyn Camera) -> anyhow::Result<(FramePool, u64)> {
    let settings = camera.settings().await?;
    let (_, width, height) = camera.capabilities().frame_shape(&settings)?;
    let bytes = settings.format.frame_bytes(width, height);
    Ok((FramePool::new(bytes, 24, &HeapAllocator), bytes as u64))
}

fn report(stats: &RecorderStats) {
    println!(
        "{:>7.1} s {:>7} frames {:>8.2} GB {:>5.0} MB/s   dropped {}   queue at most {} of {QUEUE}",
        stats.seconds,
        stats.frames,
        stats.bytes as f64 / 1e9,
        stats.bytes as f64 / 1e6 / stats.seconds,
        stats.dropped,
        stats.queued_max
    );
}

/// The bytes of the files under `folder`.
fn size(folder: &Path) -> anyhow::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        total += match entry.file_type()?.is_dir() {
            true => size(&entry.path())?,
            false => entry.metadata()?.len(),
        };
    }
    Ok(total)
}

async fn run() -> anyhow::Result<()> {
    let args = args()?;
    let temporary = args.smoke.then(tempfile::tempdir).transpose()?;
    let root = match &temporary {
        Some(folder) => folder.path().to_owned(),
        None => PathBuf::from(".oscope").join("ex-record"),
    };
    let scratch_root = args.scratch.clone().unwrap_or_else(|| root.join("scratch"));
    let data = root.join("data");
    let scratch = Arc::new(Scratch::open(&scratch_root, ScratchPolicy::default())?);
    let health = scratch.health()?;
    println!(
        "The scratch space, {}: {:.0} GB free ({:?}).",
        scratch_root.display(),
        health.available_bytes as f64 / 1e9,
        health.state
    );

    // The camera, publishing on a bus, and the recorder subscribed to it.
    let (camera, source): (Box<dyn Camera>, String) = match (args.smoke, args.test_sensor) {
        (true, _) => (
            Box::new(test_sensor(4)),
            "the test sensor, a quarter size".to_owned(),
        ),
        (false, true) => (Box::new(test_sensor(1)), "the test sensor".to_owned()),
        (false, false) => {
            let (camera, renders) = simulated().await?;
            (
                Box::new(camera),
                format!("the simulated MIChrome 20, rendering on {renders}"),
            )
        }
    };
    let seconds = if args.smoke { 1.5 } else { args.seconds };
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(100));
    let options = RecordingOptions {
        name: format!("{seconds} s of {source}"),
        source: json!({ "camera": camera.capabilities().model, "mode": "design" }),
        keep_bytes: args.keep_gb.map(|gb| (gb * 1e9) as u64),
        ..RecordingOptions::default()
    };
    // The camera streams first, as for the preview: the simulator's first
    // frame renders its field, which takes seconds.
    let (pool, frame_bytes) = pool(camera.as_ref()).await?;
    camera.start(pool, acquisition.sink()).await?;
    let warming = Instant::now();
    while bus.published() < 2 {
        ensure!(
            warming.elapsed() < Duration::from_secs(120),
            "no frame came in 2 minutes"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let recorder = Recorder::start(&bus, &scratch, options, QUEUE)?;
    println!(
        "Recording {source}: frames of {:.1} MB at 14.5 fps, {:.0} MB/s, for {seconds} s.",
        frame_bytes as f64 / 1e6,
        frame_bytes as f64 * 14.5 / 1e6
    );
    let started = Instant::now();
    let every = if seconds > 60.0 { 10.0 } else { 1.0 };
    let mut next = every;
    while started.elapsed().as_secs_f64() < seconds && recorder.stats().error.is_none() {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if started.elapsed().as_secs_f64() >= next {
            report(&recorder.stats());
            next += every;
        }
    }
    camera.stop().await?;
    let stats = recorder.stats();
    let recording = tokio::task::spawn_blocking(move || recorder.stop()).await??;
    if let Some(error) = &recording.error {
        bail!("the recording ended early: {error}");
    }
    println!(
        "Recorded {} frames, {:.2} GB, at {:.0} MB/s; {} dropped.",
        recording.frames,
        recording.bytes as f64 / 1e9,
        stats.bytes as f64 / 1e6 / stats.seconds,
        recording.dropped
    );
    if recording.dropped > 0 && source.contains("CPU") {
        println!(
            "  The simulator renders about 10 frames a second on the CPU, below the design rate: \
             build with --features cuda, or record --test-sensor."
        );
    }
    if recording.discarded > 0 {
        println!(
            "Rolling, it deleted its {} oldest frames to stay within {} GB, so it isn't ingested: \
             it is in {}.",
            recording.discarded,
            args.keep_gb.unwrap_or_default(),
            scratch.folder(recording.id)?.display()
        );
        return Ok(());
    }

    // Ingested into a new project, as a job.
    let store = Arc::new(Store::open(&data, &data.join("index.sqlite"))?);
    let project = NewProject {
        name: "ex-record".to_owned(),
        sample: "simulated H&E".to_owned(),
        tags: vec!["recording".to_owned()],
        owner: "ex-record".to_owned(),
        metadata: Map::new(),
    };
    let project = store.create(project)?;
    let jobs = JobService::open(&data.join("index.sqlite"), JobsConfig::default())?;
    jobs.register(
        INGEST,
        ingest_jobs(Arc::clone(&store), Arc::clone(&scratch)),
    );
    let params = json!({ "recording": recording.id, "project": project.id });
    let id = jobs.submit_kind(INGEST, params, None)?;
    let started = Instant::now();
    let (mut next, mut said) = (1.0, String::new());
    let job = loop {
        let job = jobs.get(id)?;
        if job.state.is_finished() {
            break job;
        }
        if started.elapsed().as_secs_f64() >= next && job.progress.stage != said {
            println!("  ingesting: {}", job.progress.stage);
            said.clone_from(&job.progress.stage);
            next += 1.0;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    ensure!(
        job.state == JobState::Completed,
        "the ingest {:?}: {}",
        job.state,
        job.error.unwrap_or_default()
    );
    let seconds = started.elapsed().as_secs_f64();
    let contents = store.contents(project.id)?;
    let acquisition = contents.acquisitions.first().context("no acquisition")?;
    let image = &acquisition.image;
    let folder = store
        .folder(project.id)
        .join("acquisitions")
        .join(acquisition.id.to_string())
        .join("raw.ome.zarr");
    let stored = size(&folder)?;
    println!(
        "Ingested in {seconds:.1} s, {:.0} MB/s: {} planes of {} × {} in {} levels, {:.2} GB \
         compressed from {:.2} GB.",
        recording.bytes as f64 / 1e6 / seconds,
        image.planes,
        image.width,
        image.height,
        image.levels,
        stored as f64 / 1e9,
        recording.bytes as f64 / 1e9
    );
    println!("The acquisition: {}", folder.display());

    if args.smoke {
        check(&scratch, &recording, image.planes, &folder)?;
        println!("ex-record --smoke: ok");
    }
    Ok(())
}

/// The smoke's checks: no frame dropped; every frame in the acquisition,
/// the first as the test sensor drew it; the recording gone.
fn check(
    scratch: &Scratch,
    recording: &oscope_store::Recording,
    planes: u32,
    folder: &Path,
) -> anyhow::Result<()> {
    ensure!(
        recording.frames >= 14,
        "{} frames in 1.5 s",
        recording.frames
    );
    ensure!(
        recording.dropped == 0,
        "{} frames dropped",
        recording.dropped
    );
    ensure!(u64::from(planes) == recording.frames, "{planes} planes");
    let (width, height) = (5472 / 4, 3648 / 4);
    let read = ImageReader::open(folder)?.read(0, 0, (0, 0), (width, height))?;
    let Samples::U16(samples) = read else {
        bail!("not 16-bit samples");
    };
    // Row y of the sensor's frame n is (y + n) mod 256, times 257: the
    // recording started at a frame after the first.
    let n = usize::from(samples[0] / 257);
    for (y, row) in samples.chunks_exact(width as usize).enumerate() {
        let level = ((y + n) % 256) as u16 * 257;
        ensure!(
            row.iter().all(|&s| s == level),
            "row {y} of the first frame"
        );
    }
    ensure!(
        scratch.health()?.recordings == 0,
        "the recording is still there"
    );
    Ok(())
}
