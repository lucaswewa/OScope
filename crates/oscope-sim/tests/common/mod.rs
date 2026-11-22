//! What the camera's tests share: a camera over an exact stage, and
//! streaming into a channel.

// Each test file uses only some of it.
#![allow(dead_code)]

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, RwLock};

use oscope_core::{FrameBuffer, FrameMeta, FramePool, HeapAllocator, Ms, Point2, Um, Xyz};
use oscope_hal::{Camera, CameraSettings, FrameSink, ObjectiveCatalogue};
use oscope_sim::{
    Optics, Rate, Settling, SimCamera, SimCameraConfig, SimClock, SimStage, SimStageConfig, Slide,
    TestTarget, World,
};

/// The slide's height: where the stage is in focus.
pub const FOCUS: f64 = 1_000.0;

pub struct Rig {
    pub clock: Arc<SimClock>,
    pub stage: Arc<SimStage>,
    pub camera: SimCamera,
}

fn optics() -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(2.4))
}

/// A camera of `config` over an exact stage at `at`, looking at the test
/// target, or at nothing, on `rate`. The target's squares lie from
/// (30, 10) mm on the slide, which sits at the stage's origin.
pub fn rig(config: SimCameraConfig, target: bool, at: Point2, rate: Rate) -> Rig {
    let world = if target {
        let target = TestTarget::new(Point2::new(30_000.0, 10_000.0), 0.5, 0.0);
        World::with_slide(
            Slide::new(Arc::new(target), Point2::new(0.0, 0.0)),
            Um(FOCUS),
        )
    } else {
        World::empty()
    };
    rig_over(config, world, at, rate)
}

/// A camera of `config` over an exact stage at `at`, looking at `world`, on
/// `rate`.
pub fn rig_over(config: SimCameraConfig, world: World, at: Point2, rate: Rate) -> Rig {
    let clock = Arc::new(SimClock::new(rate));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            acceleration: None,
            backlash: Um(0.0),
            settling: Settling {
                overshoot: Um(0.0),
                time: Ms(15.0),
            },
            require_homing: false,
            start: Xyz::new(Um(at.x), Um(at.y), Um(FOCUS)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let camera = SimCamera::new(
        config,
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics(),
    );
    Rig {
        clock,
        stage,
        camera,
    }
}

pub async fn set(camera: &SimCamera, change: impl FnOnce(&mut CameraSettings)) -> CameraSettings {
    let mut settings = camera.settings().await.unwrap();
    change(&mut settings);
    camera.configure(settings).await.unwrap()
}

/// Starts streaming into a channel.
pub async fn start(camera: &SimCamera) -> Receiver<(FrameBuffer, FrameMeta)> {
    let settings = camera.settings().await.unwrap();
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    let (sender, frames) = channel();
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        let _ = sender.send((buffer, meta));
    });
    camera.start(pool, sink).await.unwrap();
    frames
}
