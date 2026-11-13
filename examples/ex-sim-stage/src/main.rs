//! `ex-sim-stage`: the simulated grblHAL stage on the simulation clock. It
//! homes, moves, passes trigger positions, and jogs, printing each motion's
//! planned and measured duration; then it serves a page plotting the
//! stage's reported position against time. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_core::{Clock, Timestamp, Um, Xyz};
use oscope_hal::{Axis, Stage, UmPerS};
use oscope_sim::{Rate, Segment, SimClock, SimStage, SimStageConfig};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-stage: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn mm(x: f64, y: f64, z: f64) -> Xyz {
    Xyz::new(Um::from_mm(x), Um::from_mm(y), Um::from_mm(z))
}

fn seconds(t: Timestamp) -> f64 {
    t.0 as f64 / 1e9
}

/// One motion: what it was, when it ran, and how long it was planned and
/// measured to take.
struct Motion {
    what: String,
    start: Timestamp,
    end: Timestamp,
    planned: Duration,
}

impl Motion {
    fn measured(&self) -> Duration {
        Duration::from_nanos(self.end.0 - self.start.0)
    }

    fn json(&self) -> Value {
        json!({
            "what": self.what, "start": seconds(self.start), "end": seconds(self.end),
            "planned": self.planned.as_secs_f64(), "measured": self.measured().as_secs_f64(),
        })
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-sim-stage", 8217, &["--rate"])?;
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 20 } else { 10 }) as f64;
    let clock = Arc::new(SimClock::new(Rate::Scaled(rate)));
    let config = SimStageConfig::default();
    let (accel, speed, seek) = (
        config.acceleration,
        config.capabilities.default_speed.0,
        config.seek_speed.0,
    );
    let stage = Arc::new(SimStage::new(config.clone(), Arc::clone(&clock)));
    // A live stage delivers its reports as they arrive.
    tokio::spawn({
        let stage = Arc::clone(&stage);
        async move { stage.run().await }
    });
    let plan = |from: Xyz, to: Xyz, speed: f64| {
        Segment::new(from, to, Timestamp(0), speed, accel).duration()
    };
    let host = Instant::now();
    let mut motions = Vec::new();

    // Homing: z up to its switch and off it, then x and y down to theirs
    // and off them.
    stage.clear_alarm().await?;
    let start = config.start;
    let legs = [
        (start, Xyz::new(start.x, start.y, Um::from_mm(21.0))),
        (
            Xyz::new(start.x, start.y, Um::from_mm(21.0)),
            Xyz::new(start.x, start.y, Um::from_mm(20.0)),
        ),
        (
            Xyz::new(start.x, start.y, Um::from_mm(20.0)),
            mm(-1.0, -1.0, 20.0),
        ),
        (mm(-1.0, -1.0, 20.0), mm(0.0, 0.0, 20.0)),
    ];
    let began = clock.now();
    let homed = stage.home().await?;
    motions.push(Motion {
        what: "home".into(),
        start: began,
        end: homed.time,
        planned: legs.iter().map(|(a, b)| plan(*a, *b, seek)).sum(),
    });

    // Moves: long at the default speed, fast, short, and a focus move; the
    // second passes three armed trigger positions.
    stage
        .arm_trigger(Axis::X, [30.0, 45.0, 60.0].map(Um::from_mm).to_vec())
        .await?;
    let moves = [
        ("to (20, 15) mm", mm(20.0, 15.0, 20.0), None),
        (
            "to (80, 60) mm at 25 mm/s",
            mm(80.0, 60.0, 20.0),
            Some(25_000.0),
        ),
        ("by 0.2 mm in x", mm(80.2, 60.0, 20.0), None),
        ("down 5 mm in z", mm(80.2, 60.0, 15.0), None),
    ];
    let mut here = stage.position().await?.position;
    for (what, target, fast) in moves {
        let began = clock.now();
        let reached = stage.move_to(target, fast.map(UmPerS)).await?;
        motions.push(Motion {
            what: what.into(),
            start: began,
            end: reached.time,
            planned: plan(here, target, fast.unwrap_or(speed)),
        });
        here = target;
    }

    // A jog in -x at 5 mm/s, stopped after about half a second: the host
    // decides when, so it has no plan to measure against, only a distance.
    let began = clock.now();
    stage.jog(Axis::X, false, Some(UmPerS(5_000.0))).await?;
    clock.sleep(Duration::from_millis(500)).await;
    stage.stop().await?;
    let stopped = clock.now();
    let jogged = here.x.0 - stage.true_position(stopped).x.0;
    let jog = json!({"start": seconds(began), "end": seconds(stopped), "mm": jogged / 1e3});
    let host = host.elapsed();

    println!("the simulated stage at {rate}×, its motions planned and measured:");
    println!(
        "  {:<44} {:>9} {:>9} {:>7}",
        "", "planned", "measured", "late"
    );
    for m in &motions {
        let late = m.measured().as_secs_f64() - m.planned.as_secs_f64();
        println!(
            "  {:<44} {:>7.3} s {:>7.3} s {:>4.0} ms",
            m.what,
            m.planned.as_secs_f64(),
            m.measured().as_secs_f64(),
            late * 1e3
        );
    }
    println!(
        "  jog in -x at 5 mm/s, stopped after about 0.5 s: {:.3} mm in {:.3} s",
        jogged / 1e3,
        seconds(stopped) - seconds(began)
    );
    let pulses = stage.pulses();
    for p in &pulses {
        println!(
            "  trigger pulse at x = {:.1} mm, at {:.4} s",
            p.position.0 / 1e3,
            seconds(p.time)
        );
    }
    let simulated = seconds(clock.now());
    println!(
        "{simulated:.2} s simulated in {:.2} s on the host: {:.1}×",
        host.as_secs_f64(),
        simulated / host.as_secs_f64()
    );

    // The page: the reports, the motions and the pulses.
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
    let track = stage.track();
    let mut samples = Vec::new();
    let mut at = track.oldest().context("the reports")?.time;
    let last = track.latest().context("the reports")?.time;
    while at <= last {
        if let Some(p) = track.at(at) {
            samples.push(json!([seconds(at), p.x.0 / 1e3, p.y.0 / 1e3, p.z.0 / 1e3]));
        }
        at = at + Duration::from_millis(10);
    }
    let results = json!({
        "rate": rate, "samples": samples, "jog": jog,
        "motions": motions.iter().map(Motion::json).collect::<Vec<_>>(),
        "pulses": pulses.iter().map(|p| json!({"time": seconds(p.time), "x": p.position.0 / 1e3})).collect::<Vec<_>>(),
        "interval": 0.01, "latency": 0.005,
    });
    std::fs::write(
        page.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;

    let api = Api::builder(&example.roots);
    example.serve(api, &page, smoke).await
}

/// `--smoke`: the page and its results are served; each move ends within a
/// report interval of its plan; and the trigger pulsed at its three
/// positions.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/results.json", "application/json"),
    ] {
        let file = get(address, path).await?;
        ensure!(
            file.status == 200 && file.content_type.starts_with(kind),
            "{path}"
        );
    }
    let results: Value = get(address, "/results.json").await?.json()?;
    let motions = results["motions"].as_array().context("the motions")?;
    ensure!(motions.len() == 5, "{} motions", motions.len());
    // Homing and the moves answer with the first report after they end:
    // within a report interval of the plan, and a little more for the time
    // the host took to send the command, 20 times over.
    for m in motions {
        let late = m["measured"].as_f64().unwrap_or(0.0) - m["planned"].as_f64().unwrap_or(0.0);
        ensure!((0.0..0.015).contains(&late), "{}: {late} s late", m["what"]);
    }
    let pulses: Vec<f64> = results["pulses"]
        .as_array()
        .context("the pulses")?
        .iter()
        .filter_map(|p| p["x"].as_f64())
        .collect();
    ensure!(pulses == [30.0, 45.0, 60.0], "pulses at {pulses:?}");
    ensure!(
        results["samples"].as_array().is_some_and(|s| s.len() > 100),
        "too few reports"
    );
    println!("served: the page and the reports; moves on plan; three pulses");
    Ok(())
}
