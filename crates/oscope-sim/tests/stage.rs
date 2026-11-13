//! The simulated stage on a stepped clock: moves timed exactly, limits and
//! alarms, homing, stops and holds, backlash, lost steps, settling, the
//! reports, the trigger output, and the HAL's conformance suite.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use oscope_core::{Clock, Ms, Timestamp, Um, Xyz};
use oscope_hal::{Axis, Error, Stage, StageStatus, UmPerS, conformance};
use oscope_sim::{LostSteps, Rate, Segment, Settling, SimClock, SimStage, SimStageConfig};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn mm(x: f64, y: f64, z: f64) -> Xyz {
    Xyz::new(Um::from_mm(x), Um::from_mm(y), Um::from_mm(z))
}

/// A stage that is already homed, with no backlash, settling or lost
/// steps, and no acceleration unless asked: its moves are exact.
fn exact(acceleration: Option<f64>) -> SimStageConfig {
    SimStageConfig {
        acceleration,
        backlash: Um(0.0),
        settling: Settling {
            overshoot: Um(0.0),
            time: Ms(15.0),
        },
        require_homing: false,
        start: mm(10.0, 10.0, 5.0),
        ..SimStageConfig::default()
    }
}

fn stage(config: SimStageConfig) -> (Arc<SimClock>, Arc<SimStage>) {
    let clock = Arc::new(SimClock::new(Rate::Stepped));
    let stage = Arc::new(SimStage::new(config, Arc::clone(&clock)));
    (clock, stage)
}

/// Runs `work`, advancing the stepped clock a millisecond at a time until
/// it is done.
async fn drive<T: Send + 'static>(
    clock: &SimClock,
    work: impl Future<Output = T> + Send + 'static,
) -> T {
    let task = tokio::spawn(work);
    tokio::task::yield_now().await;
    while !task.is_finished() {
        clock.advance(Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    task.await.unwrap()
}

fn near(a: Xyz, b: Xyz, tolerance: f64) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.z, b.z)]
        .iter()
        .all(|(p, q)| (p.0 - q.0).abs() <= tolerance)
}

#[tokio::test]
async fn a_move_takes_its_planned_time() {
    for acceleration in [None, Some(100_000.0)] {
        let (clock, stage) = stage(exact(acceleration));
        let target = mm(20.0, 10.0, 5.0);
        let planned = Segment::new(
            mm(10.0, 10.0, 5.0),
            target,
            clock.now(),
            5_000.0,
            acceleration,
        );
        let s = Arc::clone(&stage);
        let reached = drive(&clock, async move {
            s.move_to(target, Some(UmPerS(5_000.0))).await
        })
        .await
        .unwrap();
        // 10 mm at 5 mm/s: 2 s, plus 50 ms of ramps with acceleration.
        let ends = planned.end();
        assert_eq!(
            ends.0,
            if acceleration.is_some() {
                2_050_000_000
            } else {
                2_000_000_000
            }
        );
        assert_eq!(reached.position, target);
        // It answers with the first report taken after the move ended.
        assert!(reached.time >= ends && reached.time < ends + Duration::from_millis(10));
        let track = stage.track();
        let halfway = Timestamp(1_000_000_000);
        assert!(near(
            track.at(halfway).unwrap(),
            planned.position(halfway),
            1e-6
        ));
        assert_eq!(track.at(ends), Some(target));
        assert_eq!(stage.status().await.unwrap(), StageStatus::Idle);
    }
}

#[tokio::test]
async fn a_move_answers_with_the_first_report_after_it_ends() {
    // However late the caller looks: here the clock jumps 5 s past a move
    // of 1 s, and the answer is still the report taken at 1 s.
    let (clock, stage) = stage(exact(None));
    let s = Arc::clone(&stage);
    let moving = tokio::spawn(async move { s.move_to(mm(20.0, 10.0, 5.0), None).await });
    tokio::task::yield_now().await;
    clock.advance(Duration::from_secs(5));
    let reached = moving.await.unwrap().unwrap();
    assert_eq!(reached.time, Timestamp(1_000_000_000));
    assert_eq!(reached.position, mm(20.0, 10.0, 5.0));
}

#[tokio::test]
async fn a_multi_axis_move_keeps_the_path_speed() {
    let (clock, stage) = stage(exact(None));
    let s = Arc::clone(&stage);
    // A 3-4-5 path of 5 mm at 5 mm/s: 1 s, and halfway at 0.5 s.
    drive(&clock, async move {
        s.move_to(mm(13.0, 14.0, 5.0), Some(UmPerS(5_000.0))).await
    })
    .await
    .unwrap();
    let halfway = stage.track().at(Timestamp(500_000_000)).unwrap();
    assert!(near(halfway, mm(11.5, 12.0, 5.0), 1e-6));
}

#[tokio::test]
async fn soft_limits_refuse_and_jogs_stop_at_them() {
    let (clock, stage) = stage(exact(Some(100_000.0)));
    let error = stage.move_to(mm(101.0, 10.0, 5.0), None).await.unwrap_err();
    assert!(matches!(error, Error::OutOfRange { .. }));
    stage
        .jog(Axis::X, true, Some(UmPerS(20_000.0)))
        .await
        .unwrap();
    assert_eq!(stage.status().await.unwrap(), StageStatus::Moving);
    let s = Arc::clone(&stage);
    drive(&clock, async move {
        while s.status().await.unwrap() != StageStatus::Idle {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(near(
        stage.true_position(clock.now()),
        mm(100.0, 10.0, 5.0),
        1e-6
    ));
}

#[tokio::test]
async fn a_hard_limit_raises_an_alarm_until_the_stage_is_homed() {
    // Switched on at x = 90 mm, the stage believes it is at its home
    // corner, x = 0; a move to x = 50 mm drives it truly to the switch at
    // 101 mm.
    let config = SimStageConfig {
        start: mm(90.0, 37.5, 10.0),
        ..exact(Some(100_000.0))
    };
    let config = SimStageConfig {
        require_homing: true,
        ..config
    };
    let (clock, stage) = stage(config);
    let error = stage.move_to(mm(50.0, 0.0, 20.0), None).await.unwrap_err();
    assert_eq!(error.to_string(), "the stage is in alarm: homing required");
    stage.clear_alarm().await.unwrap();
    let s = Arc::clone(&stage);
    let crash = drive(
        &clock,
        async move { s.move_to(mm(50.0, 0.0, 20.0), None).await },
    )
    .await;
    assert_eq!(
        crash.unwrap_err().to_string(),
        "the stage is in alarm: hard limit"
    );
    assert!((stage.true_position(clock.now()).x.0 - 101_000.0).abs() < 1e-6);
    assert!(stage.move_to(mm(10.0, 0.0, 20.0), None).await.is_err());

    let s = Arc::clone(&stage);
    let homed = drive(&clock, async move { s.home().await }).await.unwrap();
    assert!(near(homed.position, mm(0.0, 0.0, 20.0), 1e-6));
    assert_eq!(stage.status().await.unwrap(), StageStatus::Idle);
}

#[tokio::test]
async fn homing_seeks_each_switch_and_pulls_off() {
    let config = SimStageConfig {
        require_homing: true,
        start: mm(50.0, 37.5, 10.0),
        ..exact(Some(100_000.0))
    };
    let (clock, stage) = stage(config);
    let s = Arc::clone(&stage);
    let homed = drive(&clock, async move { s.home().await }).await.unwrap();
    assert!(near(homed.position, mm(0.0, 0.0, 20.0), 1e-6));
    // z up to its switch 1 mm past 20 mm and back; then x and y down to
    // theirs 1 mm below 0, and back: four segments at 5 mm/s.
    let a = Some(100_000.0);
    let legs = [
        (mm(50.0, 37.5, 10.0), mm(50.0, 37.5, 21.0)),
        (mm(50.0, 37.5, 21.0), mm(50.0, 37.5, 20.0)),
        (mm(50.0, 37.5, 20.0), mm(-1.0, -1.0, 20.0)),
        (mm(-1.0, -1.0, 20.0), mm(0.0, 0.0, 20.0)),
    ];
    let planned: Duration = legs
        .iter()
        .map(|(from, to)| Segment::new(*from, *to, Timestamp(0), 5_000.0, a).duration())
        .sum();
    let lag = Duration::from_nanos(homed.time.0) - planned;
    assert!(lag < Duration::from_millis(10), "{lag:?}");
}

#[tokio::test]
async fn a_stop_ends_a_move_and_a_hold_waits_for_resume() {
    let (clock, stage) = stage(exact(Some(100_000.0)));
    let s = Arc::clone(&stage);
    let moving = tokio::spawn(async move { s.move_to(mm(60.0, 10.0, 5.0), None).await });
    tokio::task::yield_now().await;
    clock.advance(Duration::from_secs(1));
    let s = Arc::clone(&stage);
    drive(&clock, async move { s.stop().await }).await.unwrap();
    assert!(matches!(moving.await.unwrap(), Err(Error::Stopped)));
    // After 1 s: 0.5 mm reaching 10 mm/s in 0.1 s, then 9 mm at it; then
    // 0.5 mm to slow down.
    let here = stage.true_position(clock.now());
    assert!(
        (here.x.0 - (10_000.0 + 9_500.0 + 500.0)).abs() < 1e-6,
        "{here:?}"
    );

    let s = Arc::clone(&stage);
    let moving = tokio::spawn(async move { s.move_to(mm(60.0, 10.0, 5.0), None).await });
    tokio::task::yield_now().await;
    clock.advance(Duration::from_secs(1));
    let s = Arc::clone(&stage);
    drive(&clock, async move { s.hold().await }).await.unwrap();
    assert_eq!(stage.status().await.unwrap(), StageStatus::Hold);
    stage.resume().await.unwrap();
    let reached = drive(&clock, async move { moving.await.unwrap() })
        .await
        .unwrap();
    assert_eq!(reached.position, mm(60.0, 10.0, 5.0));
}

#[tokio::test]
async fn while_moving_the_backlash_lags_by_half() {
    let config = SimStageConfig {
        backlash: Um(2.0),
        ..exact(None)
    };
    let (clock, stage) = stage(config);
    // From x = 10 mm towards 30 mm at 10 mm/s: at 1 s, commanded to 20 mm.
    let s = Arc::clone(&stage);
    let moving = tokio::spawn(async move { s.move_to(mm(30.0, 10.0, 5.0), None).await });
    tokio::task::yield_now().await;
    clock.advance(Duration::from_secs(1));
    let x = stage.true_position(clock.now()).x.0;
    assert!((x - (20_000.0 - 1.0)).abs() < 1e-6, "{x}");
    clock.advance(Duration::from_secs(2));
    moving.await.unwrap().unwrap();
}

#[tokio::test]
async fn backlash_is_a_dead_band_that_reversals_cross() {
    // Property: after any move, the true position lags the commanded one by
    // half the backlash, against the last direction of motion; a reversal
    // shorter than the backlash doesn't move it at all.
    let b = 2.0;
    let config = SimStageConfig {
        backlash: Um(b),
        ..exact(None)
    };
    let (clock, stage) = stage(config);
    let mut rng = StdRng::seed_from_u64(3);
    let mut here: f64 = 10_000.0;
    let mut truth = stage.true_position(clock.now()).x.0;
    for _ in 0..60 {
        let step = if rng.random_bool(0.3) {
            rng.random_range(-1.9..1.9)
        } else {
            rng.random_range(-3_000.0..3_000.0)
        };
        let target = (here + step).clamp(1_000.0, 99_000.0);
        let s = Arc::clone(&stage);
        let to = Xyz::new(Um(target), Um(10_000.0), Um(5_000.0));
        drive(&clock, async move { s.move_to(to, None).await })
            .await
            .unwrap();
        let now = stage.true_position(clock.now()).x.0;
        assert!((now - target).abs() <= b / 2.0 + 1e-9, "{now} for {target}");
        if (target - here).abs() >= b {
            let lag = if target > here { -b / 2.0 } else { b / 2.0 };
            assert!((now - (target + lag)).abs() < 1e-9);
        } else if (now - truth).abs() > 1e-9 {
            // A short reversal moves it only if it crosses the dead band.
            assert!(((now - target).abs() - b / 2.0).abs() < 1e-9);
        }
        here = target;
        truth = now;
    }
}

#[tokio::test]
async fn lost_steps_show_on_the_encoders_and_not_without_them() {
    for encoders in [true, false] {
        let mut config = SimStageConfig {
            lost_steps: LostSteps {
                chance: 1.0,
                size: Um(5.0),
            },
            ..exact(None)
        };
        config.capabilities.encoders = encoders;
        let (clock, stage) = stage(config);
        for k in 1..=4 {
            let target = mm(10.0 + 5.0 * f64::from(k), 10.0, 5.0);
            let s = Arc::clone(&stage);
            let reported = drive(&clock, async move { s.move_to(target, None).await })
                .await
                .unwrap();
            let behind = if encoders { 5.0 * f64::from(k) } else { 0.0 };
            assert!(
                (reported.position.x.0 - (target.x.0 - behind)).abs() < 1e-6,
                "move {k}"
            );
        }
    }
}

#[tokio::test]
async fn the_stage_settles_after_a_move() {
    let config = SimStageConfig {
        settling: Settling {
            overshoot: Um(0.3),
            time: Ms(15.0),
        },
        ..exact(None)
    };
    let (clock, stage) = stage(config);
    let s = Arc::clone(&stage);
    drive(&clock, async move {
        s.move_to(mm(20.0, 10.0, 5.0), Some(UmPerS(5_000.0))).await
    })
    .await
    .unwrap();
    let end = Timestamp(2_000_000_000);
    let past = |ms: u64| stage.true_position(end + Duration::from_millis(ms)).x.0 - 20_000.0;
    assert!((past(0) - 0.3).abs() < 1e-9);
    assert!((past(15) - 0.3 / std::f64::consts::E).abs() < 1e-9);
    assert!(past(150) < 1e-4);
}

#[tokio::test]
async fn reports_arrive_every_interval_after_the_latency() {
    let (clock, stage) = stage(exact(None));
    clock.advance_to(Timestamp(1_003_000_000));
    stage.pump(clock.now());
    // The report taken at 1.000 s arrives at 1.005 s.
    assert_eq!(stage.track().latest().unwrap().time, Timestamp(990_000_000));
    clock.advance_to(Timestamp(1_005_000_000));
    stage.pump(clock.now());
    assert_eq!(
        stage.track().latest().unwrap().time,
        Timestamp(1_000_000_000)
    );
    assert!(stage.track().len() >= 100);
}

#[tokio::test]
async fn the_trigger_output_pulses_at_its_positions() {
    let (clock, stage) = stage(exact(None));
    let at = |mm: f64| Um::from_mm(mm);
    stage
        .arm_trigger(Axis::X, vec![at(20.0), at(30.0), at(40.0)])
        .await
        .unwrap();
    let s = Arc::clone(&stage);
    let moving = tokio::spawn(async move { s.move_to(mm(50.0, 10.0, 5.0), None).await });
    tokio::task::yield_now().await;
    // At 10 mm/s from 10 mm: 20 mm at 1 s, 30 at 2 s, 40 at 3 s. Stopped
    // at 2.5 s, the third is taken back.
    clock.advance_to(Timestamp(2_500_000_000));
    stage.stop().await.unwrap();
    assert!(matches!(moving.await.unwrap(), Err(Error::Stopped)));
    let pulses = stage.pulses();
    let times: Vec<u64> = pulses.iter().map(|p| p.time.0).collect();
    assert_eq!(times, [1_000_000_000, 2_000_000_000]);
    assert_eq!(pulses[1].position, at(30.0));
    // A second later, the next move passes 40 mm and fires it: at 4.5 s,
    // 5 mm from where it stopped. The pulse taken back isn't kept.
    clock.advance_to(Timestamp(4_000_000_000));
    let s = Arc::clone(&stage);
    drive(
        &clock,
        async move { s.move_to(mm(50.0, 10.0, 5.0), None).await },
    )
    .await
    .unwrap();
    let pulses = stage.pulses();
    let times: Vec<u64> = pulses.iter().map(|p| p.time.0).collect();
    assert_eq!(times, [1_000_000_000, 2_000_000_000, 4_500_000_000]);
    let last = pulses[2];
    assert_eq!(last.position, at(40.0));
    assert!((stage.track().at(last.time).unwrap().x.0 - 40_000.0).abs() < 1e-6);
}

#[tokio::test]
async fn the_stage_conforms() {
    let clock = Arc::new(SimClock::new(Rate::Scaled(500.0)));
    let stage = SimStage::new(SimStageConfig::default(), clock);
    // The backlash's half and the settling's overshoot.
    conformance::stage(&stage, Um(1.5)).await.unwrap();
}
