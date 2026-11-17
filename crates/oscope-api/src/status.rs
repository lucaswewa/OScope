//! The `status` Thing (P50, plan §5.19): each subsystem's health, and the
//! instrument's, rolled up for the status indicator.
//!
//! - **The subsystems:**
//!   - the camera, the stage and the lights, each asked;
//!   - the GPU, through NVML, loaded at run time;
//!   - the scratch disk: its free space, and how fast it fills;
//!   - the jobs, and the extensions.
//! - **Each** is `ok`, `degraded`, `fault` or `offline`, with a message.
//! - **The roll-up** ([`roll_up`]) is the worst each counts as:
//!   - the camera or the stage, offline or at fault, is a fault: nothing
//!     can be acquired;
//!   - a full scratch disk is a fault: nothing can be recorded;
//!   - an absent GPU counts for nothing, as the CPU does its work;
//!   - anything else that isn't ok is degraded.
//!
//! It is sampled every second. The event `changed` says when the
//! instrument's state or message changes.

use std::cmp::Reverse;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use chrono::Utc;
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use oscope_ext::Extensions;
use oscope_hal::{Error as HalError, StageStatus};
use oscope_jobs::{JobFilter, JobService, JobState};
use oscope_store::{Scratch, ScratchState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use teta_wot::prelude::*;

use crate::devices::Devices;

/// How often the subsystems are asked.
const PERIOD: Duration = Duration::from_secs(1);

/// A subsystem's or the instrument's state.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Working.
    Ok,
    /// Working, but not wholly.
    Degraded,
    /// Not working.
    Fault,
    /// Not there.
    Offline,
}

/// A subsystem's health.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Health {
    /// Which: `camera`, `stage`, `lights`, `gpu`, `disks`, `jobs` or
    /// `extensions`.
    pub subsystem: String,
    /// Its state.
    pub state: State,
    /// Why, for people.
    pub message: String,
    /// What it measured, such as the GPU's temperature.
    #[serde(default)]
    pub details: Value,
}

/// The instrument's status: its state, why, and its subsystems'.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Instrument {
    /// `ok`, `degraded` or `fault`.
    pub state: State,
    /// Why: the worst subsystem's message, or "all well".
    pub message: String,
    /// Each subsystem's health.
    pub subsystems: Vec<Health>,
}

fn health(subsystem: &str, state: State, message: impl Into<String>, details: Value) -> Health {
    Health {
        subsystem: subsystem.to_owned(),
        state,
        message: message.into(),
        details,
    }
}

/// What a subsystem in `state` counts as, for the instrument.
fn weight(subsystem: &str, state: State) -> State {
    match (subsystem, state) {
        (_, State::Ok) => State::Ok,
        ("camera" | "stage", State::Offline | State::Fault) => State::Fault,
        ("gpu", State::Offline) => State::Ok,
        ("disks", State::Fault) => State::Fault,
        _ => State::Degraded,
    }
}

/// The instrument's status from its subsystems': the worst any counts as,
/// with the first such subsystem's message.
pub fn roll_up(subsystems: Vec<Health>) -> Instrument {
    let worst = subsystems
        .iter()
        .map(|h| (weight(&h.subsystem, h.state), h))
        .min_by_key(|(w, _)| Reverse(*w));
    let (state, message) = match worst {
        Some((state, h)) if state != State::Ok => {
            (state, format!("{}: {}", h.subsystem, h.message))
        }
        _ => (State::Ok, "all well".to_owned()),
    };
    Instrument {
        state,
        message,
        subsystems,
    }
}

/// A device's error, as its health.
fn failed(subsystem: &str, error: &HalError) -> Health {
    match error {
        HalError::Disconnected => health(subsystem, State::Offline, "disconnected", Value::Null),
        error => health(subsystem, State::Fault, error.to_string(), Value::Null),
    }
}

async fn devices(devices: &Devices) -> Vec<Health> {
    let camera = match devices.camera.settings().await {
        Ok(_) if devices.camera.streaming() => {
            health("camera", State::Ok, "streaming", Value::Null)
        }
        Ok(_) => health("camera", State::Ok, "idle", Value::Null),
        Err(error) => failed("camera", &error),
    };
    let stage = match devices.stage.status().await {
        Ok(StageStatus::Alarm { reason }) => health(
            "stage",
            State::Fault,
            format!("in alarm: {reason}"),
            Value::Null,
        ),
        Ok(status) => {
            let state = serde_json::to_value(&status).unwrap_or_default();
            let said = state["state"].as_str().unwrap_or("ready").to_owned();
            health("stage", State::Ok, said, Value::Null)
        }
        Err(error) => failed("stage", &error),
    };
    let lights = match devices.lights.state().await {
        Ok(lights) => {
            let on = lights.iter().filter(|l| l.on).count();
            let said = format!("{on} of {} on", lights.len());
            health("lights", State::Ok, said, Value::Null)
        }
        Err(error) => failed("lights", &error),
    };
    vec![camera, stage, lights]
}

/// The GPU's health: its memory and temperature, through NVML.
fn gpu(nvml: &Result<Nvml, String>) -> Health {
    let nvml = match nvml {
        Ok(nvml) => nvml,
        Err(why) => return health("gpu", State::Offline, why.clone(), Value::Null),
    };
    let device = match nvml.device_by_index(0) {
        Ok(device) => device,
        Err(e) => return health("gpu", State::Offline, format!("no GPU: {e}"), Value::Null),
    };
    let name = device.name().unwrap_or_default();
    let memory = device.memory_info().ok();
    let temperature = device.temperature(TemperatureSensor::Gpu).ok();
    let mib = |bytes: u64| bytes / (1 << 20);
    let (used, total) = memory.map_or((0, 0), |m| (mib(m.used), mib(m.total)));
    let details = json!({"name": name, "memory_used_mib": used, "memory_total_mib": total, "temperature_c": temperature});
    let hot = temperature.is_some_and(|t| t >= 87);
    let full = total > 0 && used * 100 >= total * 95;
    let said = format!(
        "{name}, {} °C, {used} of {total} MiB",
        temperature.map_or("?".to_owned(), |t| t.to_string())
    );
    match (hot, full) {
        (true, _) => health("gpu", State::Degraded, format!("hot: {said}"), details),
        (_, true) => health(
            "gpu",
            State::Degraded,
            format!("its memory nearly full: {said}"),
            details,
        ),
        _ => health("gpu", State::Ok, said, details),
    }
}

/// The scratch disk's health, and how fast it fills since `before`, the
/// bytes free when last asked: or full, if a fault says so.
fn disks(scratch: &Scratch, full: bool, before: &mut Option<(u64, Instant)>) -> Health {
    let now = match scratch.health() {
        Ok(now) => now,
        Err(e) => return health("disks", State::Fault, e.to_string(), Value::Null),
    };
    let gb = |bytes: u64| bytes as f64 / 1e9;
    let filling = match before.replace((now.available_bytes, Instant::now())) {
        Some((free, when)) => {
            let seconds = when.elapsed().as_secs_f64().max(1e-3);
            free.saturating_sub(now.available_bytes) as f64 / 1e6 / seconds
        }
        None => 0.0,
    };
    let details = json!({
        "available_gb": gb(now.available_bytes),
        "total_gb": gb(now.total_bytes),
        "filling_mb_s": filling,
        "recordings": now.recordings,
    });
    let said = format!(
        "the scratch disk: {:.0} GB free, filling at {filling:.0} MB/s",
        gb(now.available_bytes)
    );
    match (full, now.state) {
        (true, _) => health(
            "disks",
            State::Fault,
            "the scratch disk is full (a fault injected)",
            details,
        ),
        (_, ScratchState::Full) => health("disks", State::Fault, format!("full: {said}"), details),
        (_, ScratchState::Low) => health("disks", State::Degraded, format!("low: {said}"), details),
        _ => health("disks", State::Ok, said, details),
    }
}

/// The jobs' health: degraded if one failed in the last ten minutes.
fn jobs(service: &JobService) -> Health {
    let count = |state: JobState| {
        let filter = JobFilter {
            state: Some(state),
            ..JobFilter::default()
        };
        service.list(&filter).unwrap_or_default()
    };
    let recently = Utc::now() - chrono::Duration::minutes(10);
    let failed: Vec<_> = count(JobState::Failed)
        .into_iter()
        .filter(|j| j.finished.is_some_and(|f| f >= recently))
        .collect();
    let (running, queued) = (
        count(JobState::Running).len(),
        count(JobState::Queued).len(),
    );
    let details = json!({"running": running, "queued": queued, "failed_recently": failed.len()});
    match failed.first() {
        Some(job) => {
            let said = format!(
                "{} failed in the last ten minutes, {} the last: {}",
                failed.len(),
                job.spec.kind,
                job.error.clone().unwrap_or_default()
            );
            health("jobs", State::Degraded, said, details)
        }
        None => health(
            "jobs",
            State::Ok,
            format!("{running} running, {queued} queued"),
            details,
        ),
    }
}

/// The extensions' health: degraded if one has a problem.
fn extensions(extensions: &Extensions) -> Health {
    let listed = extensions.list();
    let troubled: Vec<String> = listed
        .iter()
        .filter_map(|e| {
            e.problem
                .as_ref()
                .map(|p| format!("{}: {p}", e.manifest.id))
        })
        .collect();
    let loaded = listed.iter().filter(|e| e.loaded.is_some()).count();
    match troubled.first() {
        Some(first) => health(
            "extensions",
            State::Degraded,
            first.clone(),
            json!({"problems": troubled}),
        ),
        None => health(
            "extensions",
            State::Ok,
            format!("{loaded} loaded"),
            Value::Null,
        ),
    }
}

/// What the Thing asks.
struct Sources {
    devices: Option<Arc<Devices>>,
    nvml: Result<Nvml, String>,
    scratch: Option<Arc<Scratch>>,
    jobs: Option<Arc<JobService>>,
    extensions: Option<Arc<Extensions>>,
    filling: Mutex<Option<(u64, Instant)>>,
}

impl Sources {
    /// Every subsystem's health, now.
    async fn sample(&self) -> Instrument {
        let mut subsystems = match &self.devices {
            Some(d) => devices(d).await,
            None => Vec::new(),
        };
        subsystems.push(gpu(&self.nvml));
        if let Some(scratch) = &self.scratch {
            let faults = self.devices.as_ref().and_then(|d| d.simulator.as_ref());
            let full = faults.is_some_and(|s| s.faults.scratch_full.load(Ordering::Acquire));
            let mut before = self.filling.lock().unwrap_or_else(PoisonError::into_inner);
            subsystems.push(disks(scratch, full, &mut before));
        }
        if let Some(service) = &self.jobs {
            subsystems.push(jobs(service));
        }
        if let Some(loaded) = &self.extensions {
            subsystems.push(extensions(loaded));
        }
        roll_up(subsystems)
    }
}

/// The instrument's status, and each subsystem's health.
#[derive(Thing)]
#[thing(
    title = "Status",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct StatusThing {
    /// The instrument's status changed: its state, or why.
    ///
    /// The status now.
    #[event]
    changed: Event<Instrument>,
    current: Arc<Mutex<Option<Instrument>>>,
}

#[thing_impl]
impl StatusThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let nvml = tokio::task::spawn_blocking(|| {
            Nvml::init().map_err(|e| format!("no NVIDIA driver: {e}"))
        });
        let sources = Arc::new(Sources {
            devices: server.service::<Devices>(),
            nvml: nvml.await.unwrap_or_else(|e| Err(e.to_string())),
            scratch: server.service::<Scratch>(),
            jobs: server.service::<JobService>(),
            extensions: server.service::<Extensions>(),
            filling: Mutex::new(None),
        });
        let first = sources.sample().await;
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Some(first);
        let (current, changed) = (Arc::clone(&self.current), self.changed.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PERIOD).await;
                let now = sources.sample().await;
                let mut kept = current.lock().unwrap_or_else(PoisonError::into_inner);
                let differs = kept
                    .as_ref()
                    .is_none_or(|k| (k.state, &k.message) != (now.state, &now.message));
                *kept = Some(now.clone());
                drop(kept);
                if differs {
                    changed.emit(now);
                }
            }
        });
        Ok(())
    }

    /// The instrument's status: `ok`, `degraded` or `fault`, why, and each
    /// subsystem's health, as last asked, each second.
    #[property]
    async fn instrument(&self) -> Result<Instrument, PropertyError> {
        let kept = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        kept.ok_or_else(|| PropertyError::failed(std::io::Error::other("not asked yet")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(subsystem: &str, state: State) -> Health {
        health(
            subsystem,
            state,
            format!("{subsystem} is {state:?}"),
            Value::Null,
        )
    }

    #[test]
    fn the_roll_up_is_the_worst_each_subsystem_counts_as() {
        let all = |states: &[(&str, State)]| {
            let rolled = roll_up(states.iter().map(|(s, t)| of(s, *t)).collect());
            (rolled.state, rolled.message)
        };
        assert_eq!(all(&[]), (State::Ok, "all well".to_owned()));
        assert_eq!(
            all(&[("camera", State::Ok), ("gpu", State::Offline)]).0,
            State::Ok
        );
        // The camera or the stage, offline or at fault: a fault.
        for device in ["camera", "stage"] {
            for state in [State::Offline, State::Fault] {
                assert_eq!(
                    all(&[(device, state)]).0,
                    State::Fault,
                    "{device} {state:?}"
                );
            }
            assert_eq!(all(&[(device, State::Degraded)]).0, State::Degraded);
        }
        // The lights, jobs and extensions only degrade it; a full disk is a
        // fault, and a hot GPU degrades it.
        for subsystem in ["lights", "jobs", "extensions"] {
            assert_eq!(all(&[(subsystem, State::Offline)]).0, State::Degraded);
            assert_eq!(all(&[(subsystem, State::Fault)]).0, State::Degraded);
        }
        assert_eq!(all(&[("disks", State::Fault)]).0, State::Fault);
        assert_eq!(all(&[("disks", State::Degraded)]).0, State::Degraded);
        assert_eq!(all(&[("gpu", State::Degraded)]).0, State::Degraded);
        assert_eq!(all(&[("gpu", State::Fault)]).0, State::Degraded);
        // The worst wins, and the first of the worst says why.
        let mixed = all(&[
            ("lights", State::Offline),
            ("stage", State::Fault),
            ("camera", State::Offline),
        ]);
        assert_eq!(mixed, (State::Fault, "stage: stage is Fault".to_owned()));
    }
}
