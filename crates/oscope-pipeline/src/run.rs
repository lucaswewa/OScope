//! The engine: plans run, each step once its inputs are there, independent
//! branches in parallel, and results memoised.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use oscope_compute::{Compute, Lane};
use rayon::prelude::*;
use serde::Serialize;

use crate::memo::{Memo, MemoStats, content_key, mix};
use crate::node::{Device, Inputs, NodeError, StepCtx};
use crate::plan::{Plan, Source};
use crate::recipe::{Recipe, RecipeError};
use crate::registry::Registry;
use crate::value::{PortType, Value, Values};

/// Runs plans (ADR-0017): whole ([`run`](Self::run)), tile by tile
/// ([`run_tiled`](Self::run_tiled)), per frame ([`crate::Live`]), or over a
/// dataset as a job ([`batch_job`](Self::batch_job)).
pub struct Engine {
    registry: Registry,
    pub(crate) compute: Arc<Compute>,
    memo: Memo,
}

/// How a run goes.
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// The pool it runs in: batch by default, live for live mode.
    pub lane: Lane,
    /// Whether steps' results are looked for in the memo cache, and kept
    /// there: yes by default, and never in live mode.
    pub memoise: bool,
    /// Set it to cancel the run: no step starts after, and steps that look
    /// stop sooner.
    pub cancel: Arc<AtomicBool>,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            lane: Lane::Batch,
            memoise: true,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// A run's outputs, and how it went.
#[derive(Clone, Debug)]
pub struct Run {
    /// The recipe's outputs, by name, in memory.
    pub outputs: Values,
    /// How it went.
    pub report: Report,
}

/// How a run went.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    /// Each step, in the plan's order.
    pub steps: Vec<StepReport>,
    /// The copies of images between memory and the GPU it made.
    pub transfers: u32,
    /// The tiles, for a tiled run.
    pub tiles: u32,
    /// The time it took, from its inputs to its outputs.
    pub time: Duration,
}

/// How a step went.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepReport {
    /// Its ID in the recipe.
    pub id: String,
    /// Where it ran.
    pub device: Device,
    /// The time it took; for a tiled run, over every tile.
    pub time: Duration,
    /// Whether its results came from the memo cache; for a tiled run, for
    /// every tile.
    pub cached: bool,
}

/// Why a run gave no outputs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// An input is missing, or isn't what the recipe takes.
    #[error("input `{0}`: {1}")]
    Input(String, String),
    /// A step failed.
    #[error("step `{step}` failed: {message}")]
    Step {
        /// The step's ID.
        step: String,
        /// Why.
        message: String,
    },
    /// The run was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// The plan can't run that way: not live, not tileable, or on a GPU
    /// that isn't there.
    #[error("{0}")]
    Unsuitable(String),
    /// Copying an image between memory and the GPU failed.
    #[error("copying an image between memory and the GPU: {0}")]
    Transfer(String),
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Engine {
    /// An engine for `registry`'s nodes, computing with `compute`, its memo
    /// cache holding up to `memo_bytes`.
    pub fn new(registry: Registry, compute: Arc<Compute>, memo_bytes: usize) -> Engine {
        let memo = Memo::new(memo_bytes);
        Engine {
            registry,
            compute,
            memo,
        }
    }

    /// Its nodes.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Whether it has a GPU for steps.
    pub fn has_gpu(&self) -> bool {
        #[cfg(feature = "cuda")]
        return self.compute.cuda().is_some();
        #[cfg(not(feature = "cuda"))]
        false
    }

    /// Checks and plans `recipe`, with this engine's GPU if it has one
    /// ([`Plan::new`]).
    pub fn plan(&self, recipe: &Recipe) -> Result<Plan, RecipeError> {
        Plan::new(&self.registry, recipe, self.has_gpu())
    }

    /// How its memo cache is doing.
    pub fn memo(&self) -> MemoStats {
        self.memo.stats()
    }

    /// Runs `plan` once, on `inputs` by the recipe's input names, in
    /// `options.lane`'s pool: each step as soon as the steps it reads have
    /// run, so independent branches run in parallel. Images move between
    /// memory and the GPU only where a step reads one made on the other
    /// side, once for each side. A value no step will read again is
    /// dropped.
    pub fn run(&self, plan: &Plan, inputs: &Values, options: &RunOptions) -> Result<Run, RunError> {
        let started = Instant::now();
        if plan.uses_gpu() && !self.has_gpu() {
            let message = "the plan has steps on a GPU, and this engine has none";
            return Err(RunError::Unsuitable(message.into()));
        }
        let given = plan.inputs.iter().map(|port| {
            let value = inputs.get(&port.name);
            let value =
                value.ok_or_else(|| RunError::Input(port.name.clone(), "it is missing".into()))?;
            if value.port_type() != port.kind {
                let message = format!(
                    "it is {}, and the recipe takes {}",
                    value.port_type(),
                    port.kind
                );
                return Err(RunError::Input(port.name.clone(), message));
            }
            Ok(Held::of(value.clone()))
        });
        let given: Vec<Held> = given.collect::<Result<_, _>>()?;
        self.compute.pools().pool(options.lane).install(|| {
            // The inputs' parts of the memo keys, hashed in parallel.
            let keys = if options.memoise {
                let value = |held: &Held| held.memory.clone().or_else(|| held.gpu.clone());
                given
                    .par_iter()
                    .map(|held| value(held).map_or(0, |v| content_key(&v)))
                    .collect()
            } else {
                Vec::new()
            };
            let count = plan.steps.len();
            let run = Exec {
                engine: self,
                plan,
                options,
                keys,
                inputs: given.into_iter().map(Mutex::new).collect(),
                slots: (0..count).map(|_| Mutex::default()).collect(),
                waiting: plan
                    .steps
                    .iter()
                    .map(|step| AtomicUsize::new(step.producers.len()))
                    .collect(),
                unread: plan
                    .steps
                    .iter()
                    .map(|step| AtomicUsize::new(step.readers.len()))
                    .collect(),
                failure: Mutex::new(None),
                reports: Mutex::new(vec![None; count]),
                transfers: AtomicU32::new(0),
            };
            run.go()?;
            let outputs = plan.outputs.iter().map(|(name, source)| {
                let value = run.fetch(*source, Device::Cpu)?;
                Ok((name.clone(), value))
            });
            let outputs = outputs.collect::<Result<Values, RunError>>()?;
            let steps = run
                .reports
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner);
            let report = Report {
                steps: steps.into_iter().flatten().collect(),
                transfers: run.transfers.into_inner(),
                tiles: 0,
                time: started.elapsed(),
            };
            Ok(Run { outputs, report })
        })
    }

    /// Copies an image to `to`, if it isn't there.
    #[cfg(feature = "cuda")]
    fn transfer(&self, value: &Value, to: Device, lane: Lane) -> Result<Value, RunError> {
        let failed = |error: oscope_compute::ComputeError| RunError::Transfer(error.to_string());
        let gpu = || {
            self.compute
                .cuda()
                .ok_or_else(|| RunError::Transfer("there is no GPU".into()))
        };
        match (value, to) {
            (Value::Image(image), Device::Cuda) => {
                let copy = gpu()?.upload(lane, image).map_err(failed)?;
                Ok(Value::DeviceImage(Arc::new(copy)))
            }
            (Value::DeviceImage(image), Device::Cpu) => {
                let copy = gpu()?.download(lane, image).map_err(failed)?;
                Ok(Value::image(copy))
            }
            _ => Ok(value.clone()),
        }
    }

    #[cfg(not(feature = "cuda"))]
    fn transfer(&self, value: &Value, _: Device, _: Lane) -> Result<Value, RunError> {
        Ok(value.clone())
    }
}

/// A value where it is, and its copy on the other side once made.
#[derive(Clone, Default)]
struct Held {
    memory: Option<Value>,
    gpu: Option<Value>,
}

impl Held {
    fn of(value: Value) -> Held {
        if value.on_gpu() {
            Held {
                memory: None,
                gpu: Some(value),
            }
        } else {
            Held {
                memory: Some(value),
                gpu: None,
            }
        }
    }

    /// The value as a step on `device` reads it: values other than images
    /// are read where they are.
    fn on(&self, device: Device) -> Option<&Value> {
        match device {
            Device::Cpu => self.memory.as_ref(),
            Device::Cuda => self.gpu.as_ref().or_else(|| {
                let memory = self.memory.as_ref()?;
                (memory.port_type() != PortType::Image).then_some(memory)
            }),
        }
    }
}

/// A step's outputs, by port, and its memo key.
#[derive(Default)]
struct Slot {
    key: u64,
    values: Vec<Held>,
}

/// One run of a plan.
struct Exec<'a> {
    engine: &'a Engine,
    plan: &'a Plan,
    options: &'a RunOptions,
    inputs: Vec<Mutex<Held>>,
    /// The recipe inputs' parts of the memo keys.
    keys: Vec<u64>,
    slots: Vec<Mutex<Slot>>,
    /// For each step, the steps it reads that haven't run.
    waiting: Vec<AtomicUsize>,
    /// For each step, the steps reading it that haven't started.
    unread: Vec<AtomicUsize>,
    failure: Mutex<Option<RunError>>,
    reports: Mutex<Vec<Option<StepReport>>>,
    transfers: AtomicU32,
}

impl Exec<'_> {
    /// Runs every step, in the pool it is called in, from those that read
    /// no step.
    fn go(&self) -> Result<(), RunError> {
        rayon::scope(|scope| {
            for (s, step) in self.plan.steps.iter().enumerate() {
                if step.producers.is_empty() {
                    scope.spawn(move |scope| self.step(scope, s));
                }
            }
        });
        match lock(&self.failure).take() {
            Some(error) => Err(error),
            None if self.options.cancel.load(Ordering::Acquire) => Err(RunError::Cancelled),
            None => Ok(()),
        }
    }

    /// Runs step `s`, then starts each step it was the last to wait for.
    fn step<'s>(&'s self, scope: &rayon::Scope<'s>, s: usize) {
        if self.stopped() {
            return;
        }
        match self.run_step(s) {
            Ok(()) => {
                for &reader in &self.plan.steps[s].readers {
                    if self.waiting[reader].fetch_sub(1, Ordering::AcqRel) == 1 {
                        scope.spawn(move |scope| self.step(scope, reader));
                    }
                }
            }
            Err(error) => {
                lock(&self.failure).get_or_insert(error);
            }
        }
    }

    fn stopped(&self) -> bool {
        self.options.cancel.load(Ordering::Acquire) || lock(&self.failure).is_some()
    }

    fn run_step(&self, s: usize) -> Result<(), RunError> {
        let planned = &self.plan.steps[s];
        let descriptor = planned.node.descriptor();
        let mut values = Vec::with_capacity(planned.sources.len());
        let mut parts = vec![planned.key];
        for source in &planned.sources {
            let (value, key) = match source {
                Some(source) => (
                    Some(self.fetch(*source, planned.device)?),
                    self.key_of(*source),
                ),
                None => (None, 0),
            };
            values.push(value);
            parts.push(key);
        }
        let key = mix(&parts);
        // What it read and nobody else will is dropped.
        for &producer in &planned.producers {
            if self.unread[producer].fetch_sub(1, Ordering::AcqRel) == 1
                && !self.plan.steps[producer].output
            {
                lock(&self.slots[producer]).values.clear();
            }
        }
        let started = Instant::now();
        let memoised = self
            .options
            .memoise
            .then(|| self.engine.memo.get(key))
            .flatten();
        let cached = memoised.is_some();
        let outputs = match memoised {
            Some(outputs) => outputs,
            None => {
                if self.options.cancel.load(Ordering::Acquire) {
                    return Err(RunError::Cancelled);
                }
                let ctx = StepCtx {
                    compute: &self.engine.compute,
                    device: planned.device,
                    lane: self.options.lane,
                    cancel: &self.options.cancel,
                };
                let inputs = Inputs {
                    ports: &descriptor.inputs,
                    values: &values,
                };
                let failed = |message: String| RunError::Step {
                    step: planned.id.clone(),
                    message,
                };
                let mut given =
                    match catch_unwind(AssertUnwindSafe(|| planned.step.run(&ctx, &inputs))) {
                        Ok(Ok(given)) => given,
                        Ok(Err(NodeError::Cancelled)) => return Err(RunError::Cancelled),
                        Ok(Err(NodeError::Failed(message))) => return Err(failed(message)),
                        Err(panic) => {
                            return Err(failed(format!("it panicked: {}", panic_text(&*panic))));
                        }
                    };
                let outputs = descriptor.outputs.iter().map(|port| {
                    let value = given.remove(&port.name);
                    let value = value
                        .ok_or_else(|| failed(format!("it gave nothing on `{}`", port.name)))?;
                    if value.port_type() != port.kind {
                        let message = format!(
                            "it gave {} on `{}`, which is {}",
                            value.port_type(),
                            port.name,
                            port.kind
                        );
                        return Err(failed(message));
                    }
                    Ok(Some(value))
                });
                let outputs = outputs.collect::<Result<Vec<_>, _>>()?;
                if self.options.memoise {
                    self.engine.memo.put(key, &outputs);
                }
                outputs
            }
        };
        let time = started.elapsed();
        let values = outputs
            .into_iter()
            .map(|value| value.map(Held::of).unwrap_or_default());
        *lock(&self.slots[s]) = Slot {
            key,
            values: values.collect(),
        };
        lock(&self.reports)[s] = Some(StepReport {
            id: planned.id.clone(),
            device: planned.device,
            time,
            cached,
        });
        Ok(())
    }

    /// The memo key of what `source` gives.
    fn key_of(&self, source: Source) -> u64 {
        match source {
            Source::Input(i) => self.keys.get(i).copied().unwrap_or_default(),
            Source::Step(s, port) => mix(&[lock(&self.slots[s]).key, port as u64]),
        }
    }

    /// The value `source` gives, where a step on `device` reads it, copied
    /// there the first time it is needed there.
    fn fetch(&self, source: Source, device: Device) -> Result<Value, RunError> {
        let mut slot;
        let mut input;
        let held = match source {
            Source::Input(i) => {
                input = lock(&self.inputs[i]);
                &mut *input
            }
            Source::Step(s, port) => {
                slot = lock(&self.slots[s]);
                let Some(held) = slot.values.get_mut(port) else {
                    return Err(RunError::Unsuitable(
                        "a step's output was dropped too soon".into(),
                    ));
                };
                held
            }
        };
        if let Some(value) = held.on(device) {
            return Ok(value.clone());
        }
        let there = held.memory.as_ref().or(held.gpu.as_ref());
        let there = there.ok_or_else(|| RunError::Unsuitable("a value is missing".into()))?;
        let copy = self.engine.transfer(there, device, self.options.lane)?;
        self.transfers.fetch_add(1, Ordering::Relaxed);
        match device {
            Device::Cpu => held.memory = Some(copy.clone()),
            Device::Cuda => held.gpu = Some(copy.clone()),
        }
        Ok(copy)
    }
}

fn panic_text(panic: &(dyn Any + Send)) -> String {
    let text = panic.downcast_ref::<&str>().map(|text| (*text).to_owned());
    text.or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "with no message".into())
}
