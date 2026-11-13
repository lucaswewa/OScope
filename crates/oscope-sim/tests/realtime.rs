//! Moves on a running clock: at 1× they take their time on the host, and at
//! 10× a tenth of it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_core::{Um, Xyz};
use oscope_hal::{Stage, UmPerS};
use oscope_sim::{Rate, Settling, SimClock, SimStage, SimStageConfig};

/// A homed stage at (10, 10, 5) mm, without acceleration, backlash or
/// settling.
fn stage(rate: Rate) -> SimStage {
    let config = SimStageConfig {
        acceleration: None,
        backlash: Um(0.0),
        settling: Settling {
            overshoot: Um(0.0),
            time: oscope_core::Ms(15.0),
        },
        require_homing: false,
        start: Xyz::new(Um(10_000.0), Um(10_000.0), Um(5_000.0)),
        ..SimStageConfig::default()
    };
    SimStage::new(config, Arc::new(SimClock::new(rate)))
}

/// How long, on the host, a 4 mm move at 10 mm/s takes: 0.4 s simulated,
/// and up to a report interval and the latency more, 15 ms, before it
/// answers.
async fn host_time(rate: Rate) -> Duration {
    let stage = stage(rate);
    let host = Instant::now();
    let target = Xyz::new(Um(14_000.0), Um(10_000.0), Um(5_000.0));
    stage.move_to(target, Some(UmPerS(10_000.0))).await.unwrap();
    host.elapsed()
}

#[tokio::test]
async fn at_1x_a_move_takes_its_time_on_the_host() {
    let took = host_time(Rate::RealTime).await;
    assert!(took >= Duration::from_millis(400), "{took:?}");
    assert!(took < Duration::from_millis(400 + 15 + 100), "{took:?}");
}

#[tokio::test]
async fn at_10x_it_takes_a_tenth() {
    let took = host_time(Rate::Scaled(10.0)).await;
    assert!(took >= Duration::from_millis(40), "{took:?}");
    assert!(took < Duration::from_millis(40 + 2 + 60), "{took:?}");
}
