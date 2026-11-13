//! The job service, which runs every long piece of work: hardware jobs one at
//! a time, and compute jobs on a pool.
//!
//! Layer L3 of ADR-0005. ADR-0016 records the design (D28, plan §5.11):
//!
//! - **Hardware jobs** run one at a time, in a queue whose order can change,
//!   and hold the hardware lock while they run ([`HardwareLock`]): the device
//!   Things refuse manual moves meanwhile.
//! - **Compute jobs** run on a pool of workers, each holding the GPU memory
//!   it says it needs, from a budget.
//! - **A job's code** reports its progress, stops at its checkpoints when
//!   cancelled, waits there while paused if it can pause, and runs in a
//!   `job` span, so that its log records carry its ID (ADR-0009)
//!   ([`JobCtx`]).
//! - **The history** is kept in SQLite. Jobs a stop cut short are marked
//!   interrupted when the service opens ([`JobStore`]).

mod ctx;
mod model;
mod service;
mod store;

pub use ctx::{Cancelled, JobCtx};
pub use model::{JobError, JobFilter, JobInfo, JobSpec, JobState, Progress, Resource, Stop};
pub use service::{Factory, HardwareLock, Job, JobFuture, JobService, JobsConfig};
pub use store::JobStore;
