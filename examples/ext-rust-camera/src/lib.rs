//! A sample Rust extension for OScope (P27, ADR-0036): a camera driver,
//! `example.fakecam`, which streams a test pattern that moves with each
//! frame. The server runs it in place of the simulator when its
//! configuration says `camera = "example.fakecam"`.
//!
//! OScope does what its HAL promises around it: it checks settings, refuses
//! what can't change while it streams, waits for software triggers, and
//! times, numbers and pools frames. So the driver only takes its settings,
//! and captures each frame when it is due.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use oscope_ext_sdk::CameraDriver;
use oscope_ext_sdk::oscope_core::{Ms, PixelFormat, Um};
use oscope_ext_sdk::oscope_hal::{
    CameraCapabilities, CameraMode, CameraSettings, ModeRate, RoiRules, Shutter,
    TriggerCapabilities,
};

/// The fake camera: a 640 × 480 sensor of 3.45 µm pixels, with a global
/// shutter, in 8 or 16 bits of grey.
#[derive(Default)]
pub struct FakeCam {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    settings: Option<CameraSettings>,
    frame: u64,
    due: Option<Instant>,
}

impl FakeCam {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The test pattern's level at sensor pixel (`x`, `y`) in frame `frame`:
/// diagonal stripes that move a pixel a frame.
pub fn level(x: u32, y: u32, frame: u64) -> u8 {
    ((u64::from(x) + u64::from(y) + frame) / 2 % 256) as u8
}

impl CameraDriver for FakeCam {
    fn capabilities(&self) -> CameraCapabilities {
        let rates = |fps8: f64| {
            vec![
                ModeRate {
                    format: PixelFormat::Mono8,
                    fps: fps8,
                },
                ModeRate {
                    format: PixelFormat::Mono16,
                    fps: fps8 / 2.0,
                },
            ]
        };
        let mode = |name: &str, binning: u32, fps8: f64| CameraMode {
            name: name.to_owned(),
            width: 640 / binning,
            height: 480 / binning,
            binning,
            rates: rates(fps8),
            line_time: None,
        };
        CameraCapabilities {
            model: "OScope's example fake camera".to_owned(),
            sensor_width: 640,
            sensor_height: 480,
            pixel_size: Um(3.45),
            shutter: Shutter::Global,
            modes: vec![mode("full", 1, 60.0), mode("bin2", 2, 120.0)],
            exposure_min: Ms(0.01),
            exposure_max: Ms(1_000.0),
            gain_min: 0.0,
            gain_max: 24.0,
            roi: Some(RoiRules {
                x_step: 2,
                y_step: 2,
                min_size: 16,
                raises_rate: false,
            }),
            trigger: TriggerCapabilities {
                software: true,
                external: false,
                output: false,
            },
            sensor_timestamps: false,
            temperature: false,
        }
    }

    fn configure(&self, settings: &CameraSettings) -> Result<(), String> {
        self.state().settings = Some(settings.clone());
        Ok(())
    }

    fn start(&self, settings: &CameraSettings) -> Result<(), String> {
        let mut state = self.state();
        state.settings = Some(settings.clone());
        state.due = None;
        Ok(())
    }

    fn capture(&self, pixels: &mut [u8]) -> Result<(), String> {
        let (settings, frame, due) = {
            let state = self.state();
            let settings = state.settings.clone().ok_or("it has no settings")?;
            (settings, state.frame, state.due)
        };
        let capabilities = self.capabilities();
        let mode = capabilities
            .mode(&settings.mode)
            .map_err(|e| e.to_string())?;
        let fps = mode.fps(settings.format).unwrap_or(1.0);
        let exposure = settings.exposure.to_duration().unwrap_or_default();
        let interval = exposure.max(Duration::from_secs_f64(1.0 / fps));
        // Each frame when it is due, as a free-running sensor delivers them.
        let now = Instant::now();
        let due = due.filter(|due| *due > now).unwrap_or(now);
        std::thread::sleep(due.saturating_duration_since(now));
        let (roi, width, height) = capabilities
            .frame_shape(&settings)
            .map_err(|e| e.to_string())?;
        let (left, top) = (roi.x, roi.y);
        let wide = settings.format == PixelFormat::Mono16;
        let sample = if wide { 2 } else { 1 };
        let needed = width as usize * height as usize * sample;
        if pixels.len() != needed {
            return Err(format!(
                "a buffer of {} bytes, for a frame of {needed}",
                pixels.len()
            ));
        }
        for (row, line) in pixels.chunks_exact_mut(width as usize * sample).enumerate() {
            let y = top + row as u32 * mode.binning;
            for (column, out) in line.chunks_exact_mut(sample).enumerate() {
                let x = left + column as u32 * mode.binning;
                let level = level(x, y, frame);
                match wide {
                    true => out.copy_from_slice(&(u16::from(level) * 257).to_le_bytes()),
                    false => out[0] = level,
                }
            }
        }
        let mut state = self.state();
        state.frame = frame + 1;
        state.due = Some(due + interval);
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        self.state().due = None;
        Ok(())
    }
}

oscope_ext_sdk::export!(
    nodes: [],
    drivers: [oscope_ext_sdk::camera("example.fakecam", FakeCam::default())],
);
