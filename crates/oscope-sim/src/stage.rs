//! The simulated stage: grblHAL-like, timed by the simulation clock (D20,
//! D30, ADR-0013).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use oscope_core::{Clock, Ms, Timestamp, Um, Xyz};
use oscope_hal::{
    Axis, Error, HalFuture, PositionSample, PositionTrack, Result, Stage, StageCapabilities,
    StageStatus, Travel, UmPerS,
};
use tokio::sync::Notify;

use crate::clock::SimClock;
use crate::motion::Segment;
use crate::noise::{hash, nth};

/// Steps lost on a move, as a stepper motor loses them under load.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LostSteps {
    /// The chance that a move loses steps.
    pub chance: f64,
    /// How far behind the stage then falls, along the move's main axis.
    pub size: Um,
}

/// How the stage settles after a move: it overshoots, and the overshoot
/// dies away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settling {
    /// The overshoot when the move ends.
    pub overshoot: Um,
    /// The time it takes to fall to 1/e of that.
    pub time: Ms,
}

/// The simulated stage's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct SimStageConfig {
    /// What it reports as its capabilities: its soft limits, speeds,
    /// homing, encoders, trigger output and report interval.
    pub capabilities: StageCapabilities,
    /// Its acceleration in µm/s², or `None` for none.
    pub acceleration: Option<f64>,
    /// The speed it homes at.
    pub seek_speed: UmPerS,
    /// How far beyond each soft limit its limit switch is, and how far it
    /// pulls off a switch after homing.
    pub pull_off: Um,
    /// Which axes home to their greatest position: z, by default, away from
    /// the sample.
    pub home_to_max: [bool; 3],
    /// How long a status report takes to arrive.
    pub latency: Ms,
    /// The play when an axis reverses.
    pub backlash: Um,
    /// Steps lost on moves.
    pub lost_steps: LostSteps,
    /// How it settles after a move.
    pub settling: Settling,
    /// Where it truly is when it is switched on. It believes it is at its
    /// home corner until it homes, as grblHAL does.
    pub start: Xyz,
    /// Whether it starts in alarm until homed, as grblHAL with homing on.
    pub require_homing: bool,
    /// The seed for lost steps.
    pub seed: u64,
}

impl Default for SimStageConfig {
    /// 100 × 75 × 20 mm of travel; 10 mm/s by default, 25 at most,
    /// accelerating at 100 mm/s²; homing at 5 mm/s, with switches 1 mm
    /// beyond the soft limits; encoders and a trigger output; a report every
    /// 10 ms, arriving 5 ms later; 2 µm of backlash; no lost steps; a 0.3 µm
    /// overshoot settling in 15 ms; switched on in the middle, needing
    /// homing.
    fn default() -> Self {
        let travel = |mm: f64| Travel {
            min: Um(0.0),
            max: Um::from_mm(mm),
        };
        Self {
            capabilities: StageCapabilities {
                model: "simulated grblHAL stage".to_owned(),
                x: travel(100.0),
                y: travel(75.0),
                z: travel(20.0),
                max_speed: UmPerS(25_000.0),
                default_speed: UmPerS(10_000.0),
                homing: true,
                encoders: true,
                trigger_output: true,
                report_interval: Ms(10.0),
            },
            acceleration: Some(100_000.0),
            seek_speed: UmPerS(5_000.0),
            pull_off: Um(1_000.0),
            home_to_max: [false, false, true],
            latency: Ms(5.0),
            backlash: Um(2.0),
            lost_steps: LostSteps {
                chance: 0.0,
                size: Um(5.0),
            },
            settling: Settling {
                overshoot: Um(0.3),
                time: Ms(15.0),
            },
            start: Xyz::new(Um(50_000.0), Um(37_500.0), Um(10_000.0)),
            require_homing: true,
            seed: 1,
        }
    }
}

/// A pulse from the trigger output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    /// When.
    pub time: Timestamp,
    /// Along which axis.
    pub axis: Axis,
    /// At which position on it.
    pub position: Um,
}

/// The stage's motion from a moment on, until the next change: what its
/// true position follows.
#[derive(Clone, Copy, Debug)]
struct Phase {
    since: Timestamp,
    motion: Option<Segment>,
    rest: Xyz,
    play: [f64; 3],
    lost: [f64; 3],
    settle: Option<(Timestamp, [f64; 3])>,
}

/// How many past phases the stage remembers: minutes of ordinary use.
const HISTORY: usize = 256;

/// What the current motion is for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Purpose {
    /// A move, with its number, target and speed.
    Move(u64, Xyz, f64),
    Jog,
    /// Slowing to a stop, then idle.
    Stop,
    /// Slowing to a feed hold, for a move with its number, target and
    /// speed.
    Hold(u64, Xyz, f64),
    /// Homing: its number, and the phase (0–3).
    Home(u64, usize),
}

/// How a move ended.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    /// At this time.
    Done(Timestamp),
    Stopped,
    Alarm(String),
}

#[derive(Debug)]
struct State {
    status: StageStatus,
    /// The commanded motion, or `None` at rest at `rest`.
    motion: Option<(Segment, Purpose)>,
    rest: Xyz,
    /// Each axis's true position less `lost`, after the backlash, at the
    /// start of the current motion.
    play: [f64; 3],
    /// The true position less the commanded one, from lost steps or, before
    /// homing, from not knowing where it is.
    lost: [f64; 3],
    /// When the last move ended, and its direction, for the overshoot.
    settle: Option<(Timestamp, [f64; 3])>,
    /// A hard limit the current motion reaches, and when.
    hard: Option<Timestamp>,
    homed: bool,
    moves: u64,
    outcomes: HashMap<u64, Outcome>,
    /// The next report's number, and the reports waiting to arrive.
    next_report: u64,
    in_flight: VecDeque<PositionSample>,
    armed: Option<(Axis, Vec<f64>)>,
    pulses: Vec<Pulse>,
    /// A held move: its number, target and speed.
    held: Option<(u64, Xyz, f64)>,
    /// When the current phase began, and the phases before it.
    since: Timestamp,
    past: VecDeque<Phase>,
}

fn axes(p: Xyz) -> [f64; 3] {
    [p.x.0, p.y.0, p.z.0]
}

fn point(a: [f64; 3]) -> Xyz {
    Xyz::new(Um(a[0]), Um(a[1]), Um(a[2]))
}

fn index(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

/// A stage that moves like grblHAL on the simulation clock (P11).
///
/// - **Motion** follows [`Segment`]s: moves, jogs, stops, feed holds and
///   homing all take the time their distances, speeds and acceleration
///   give.
/// - **Limits:** a target beyond the soft limits is refused. A limit
///   switch, `pull_off` beyond each soft limit, stops the stage at once and
///   raises an alarm, as a hard limit does. Until it homes, the stage
///   believes it is at its home corner, wherever it truly is.
/// - **The true position** differs from the commanded one by the backlash,
///   lost steps, and the overshoot while it settles. The encoders report the
///   true position; without them it reports the commanded one.
/// - **Status reports** are taken every report interval and arrive after
///   the latency, feeding its [`PositionTrack`]. Everything follows from the
///   clock, never from how often the stage is read.
/// - **The trigger output** pulses as the stage passes armed positions.
pub struct SimStage {
    config: SimStageConfig,
    clock: Arc<SimClock>,
    state: Mutex<State>,
    track: Arc<PositionTrack>,
    changed: Notify,
}

impl SimStage {
    /// A stage with `config`, on `clock`.
    pub fn new(config: SimStageConfig, clock: Arc<SimClock>) -> Self {
        let home = Self::home_corner(&config.capabilities, &config.home_to_max);
        let believed = if config.require_homing {
            home
        } else {
            config.start
        };
        let lost = [0, 1, 2].map(|i| axes(config.start)[i] - axes(believed)[i]);
        let status = if config.require_homing {
            StageStatus::Alarm {
                reason: "homing required".to_owned(),
            }
        } else {
            StageStatus::Idle
        };
        let now = clock.now();
        let interval = (config.capabilities.report_interval.0 * 1e6) as u64;
        let stage = Self {
            state: Mutex::new(State {
                status,
                motion: None,
                rest: believed,
                play: axes(believed),
                lost,
                settle: None,
                hard: None,
                homed: !config.require_homing,
                moves: 0,
                outcomes: HashMap::new(),
                next_report: now.0 / interval.max(1) + 1,
                in_flight: VecDeque::new(),
                armed: None,
                pulses: Vec::new(),
                held: None,
                since: now,
                past: VecDeque::new(),
            }),
            track: Arc::new(PositionTrack::new(4096)),
            changed: Notify::new(),
            clock,
            config,
        };
        // grblHAL answers a status report as soon as it is connected.
        let first = stage.reported(&stage.state(), now);
        stage.track.push(PositionSample {
            time: now,
            position: first,
        });
        stage
    }

    fn home_corner(caps: &StageCapabilities, to_max: &[bool; 3]) -> Xyz {
        let t = [caps.x, caps.y, caps.z];
        point([0, 1, 2].map(|i| if to_max[i] { t[i].max.0 } else { t[i].min.0 }))
    }

    /// Raises an alarm, as a fault to inject (P50): it stops where it is
    /// until the alarm is cleared.
    pub fn inject_alarm(&self, reason: &str) {
        let now = self.clock.now();
        self.pump(now);
        let mut state = self.state();
        // A motion stops, and what waits for it hears of the alarm.
        if let Some((segment, purpose)) = state.motion {
            let here = segment.position(now);
            self.rest_at(&mut state, now, here);
            state.settle = None;
            self.unfire(&mut state, now);
            if let Purpose::Move(id, ..) | Purpose::Hold(id, ..) | Purpose::Home(id, _) = purpose {
                state.outcomes.insert(id, Outcome::Alarm(reason.to_owned()));
            }
        }
        state.status = StageStatus::Alarm {
            reason: reason.to_owned(),
        };
        drop(state);
        self.changed.notify_waiters();
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn interval(&self) -> u64 {
        ((self.config.capabilities.report_interval.0 * 1e6) as u64).max(1)
    }

    fn latency(&self) -> Duration {
        Duration::from_secs_f64(self.config.latency.0 / 1e3)
    }

    /// The current phase.
    fn current(state: &State) -> Phase {
        Phase {
            since: state.since,
            motion: state.motion.map(|(segment, _)| segment),
            rest: state.rest,
            play: state.play,
            lost: state.lost,
            settle: state.settle,
        }
    }

    /// The phase that covered `at`: the current one, or a remembered one.
    fn phase(state: &State, at: Timestamp) -> Phase {
        if at >= state.since {
            return Self::current(state);
        }
        state
            .past
            .iter()
            .rev()
            .find(|p| p.since <= at)
            .or(state.past.front())
            .copied()
            .unwrap_or_else(|| Self::current(state))
    }

    /// Remembers the current phase, and starts a new one at `at`.
    fn archive(state: &mut State, at: Timestamp) {
        let phase = Self::current(state);
        state.past.push_back(phase);
        if state.past.len() > HISTORY {
            state.past.pop_front();
        }
        state.since = at;
    }

    /// The commanded position at `at`.
    fn commanded(state: &State, at: Timestamp) -> Xyz {
        match Self::phase(state, at).motion {
            Some(segment) => segment.position(at),
            None => Self::phase(state, at).rest,
        }
    }

    /// The true position at `at`, from the commanded one, the backlash,
    /// the steps lost and the settling. Exact for the present and the recent
    /// past: the stage remembers its last 256 changes of motion.
    pub fn true_position(&self, at: Timestamp) -> Xyz {
        let state = self.state();
        self.truth(&state, at)
    }

    fn truth(&self, state: &State, at: Timestamp) -> Xyz {
        let half = self.config.backlash.0 / 2.0;
        let phase = Self::phase(state, at);
        let c = axes(Self::commanded(state, at));
        let mut t =
            [0, 1, 2].map(|i| phase.play[i].clamp(c[i] - half, c[i] + half) + phase.lost[i]);
        if let Some((end, dir)) = phase
            .settle
            .filter(|(end, _)| at >= *end && phase.motion.is_none())
        {
            let s = &self.config.settling;
            let since = (at.0 - end.0) as f64 / 1e6;
            let left = s.overshoot.0 * (-since / s.time.0.max(1e-9)).exp();
            for i in 0..3 {
                t[i] += dir[i] * left;
            }
        }
        point(t)
    }

    /// What a report at `at` says: the true position with encoders, the
    /// commanded one without.
    fn reported(&self, state: &State, at: Timestamp) -> Xyz {
        if self.config.capabilities.encoders {
            self.truth(state, at)
        } else {
            Self::commanded(state, at)
        }
    }

    /// The trigger output's pulses so far.
    pub fn pulses(&self) -> Vec<Pulse> {
        let now = self.clock.now();
        self.pump(now);
        self.state()
            .pulses
            .iter()
            .filter(|p| p.time <= now)
            .copied()
            .collect()
    }

    /// Brings the stage up to the clock's time: ends motions that have
    /// ended, stops at a hard limit, and takes and delivers the reports that
    /// are due, in time order.
    pub fn pump(&self, now: Timestamp) {
        let mut state = self.state();
        loop {
            // The motion's end, or the hard limit it meets first.
            let event = state
                .motion
                .as_ref()
                .map(|(segment, _)| state.hard.unwrap_or(segment.end()).min(segment.end()));
            // Reports taken before the event, with the motion as it was;
            // one at the event's instant is taken after it.
            let until = match event {
                Some(e) if e <= now => e.0.saturating_sub(1),
                _ => now.0,
            };
            while state.next_report * self.interval() <= until {
                let at = Timestamp(state.next_report * self.interval());
                let position = self.reported(&state, at);
                state
                    .in_flight
                    .push_back(PositionSample { time: at, position });
                state.next_report += 1;
            }
            match event {
                Some(e) if e <= now => self.finish(&mut state, e),
                _ => break,
            }
        }
        let latency = self.latency();
        while let Some(sample) = state.in_flight.front().copied() {
            if sample.time + latency > now {
                break;
            }
            self.track.push(sample);
            state.in_flight.pop_front();
        }
    }

    /// Ends the current motion at `at`, its end or a hard limit.
    fn finish(&self, state: &mut State, at: Timestamp) {
        let Some((segment, purpose)) = state.motion else {
            return;
        };
        let hit = state.hard.is_some_and(|h| h <= at) && !matches!(purpose, Purpose::Home(..));
        let here = segment.position(at);
        self.rest_at(state, at, here);
        if hit {
            state.homed = false;
            state.status = StageStatus::Alarm {
                reason: "hard limit".to_owned(),
            };
            state.settle = None;
            self.unfire(state, at);
            if let Purpose::Move(id, ..) | Purpose::Hold(id, ..) | Purpose::Home(id, _) = purpose {
                state
                    .outcomes
                    .insert(id, Outcome::Alarm("hard limit".to_owned()));
            }
            return;
        }
        match purpose {
            Purpose::Move(id, ..) => {
                self.lose_steps(state, segment);
                state.settle = Some((at, segment.direction()));
                state.status = StageStatus::Idle;
                state.outcomes.insert(id, Outcome::Done(at));
            }
            Purpose::Jog | Purpose::Stop => {
                state.settle = Some((at, segment.direction()));
                state.status = StageStatus::Idle;
            }
            Purpose::Hold(id, target, speed) => {
                state.status = StageStatus::Hold;
                state.held = Some((id, target, speed));
            }
            Purpose::Home(id, phase) => {
                if phase == 1 {
                    // Off the z switch, then on to x's and y's.
                    let target = self.switches(state, [true, true, false]);
                    self.begin(
                        state,
                        at,
                        target,
                        self.config.seek_speed.0,
                        Purpose::Home(id, 2),
                    );
                } else if phase == 0 || phase == 2 {
                    let back = self.pulled_off(state, phase == 0);
                    self.begin(
                        state,
                        at,
                        back,
                        self.config.seek_speed.0,
                        Purpose::Home(id, phase + 1),
                    );
                } else {
                    state.homed = true;
                    state.settle = None;
                    state.status = StageStatus::Idle;
                    state.outcomes.insert(id, Outcome::Done(at));
                }
            }
        }
    }

    /// At rest at `here` from `at`, with the backlash carried on.
    fn rest_at(&self, state: &mut State, at: Timestamp, here: Xyz) {
        let half = self.config.backlash.0 / 2.0;
        let c = axes(Self::commanded(state, at));
        Self::archive(state, at);
        state.play = [0, 1, 2].map(|i| state.play[i].clamp(c[i] - half, c[i] + half));
        state.motion = None;
        state.hard = None;
        state.rest = here;
    }

    fn lose_steps(&self, state: &mut State, segment: Segment) {
        let lost = self.config.lost_steps;
        let h = hash(self.config.seed, 0x1057, state.moves as i64, 0);
        if nth(h, 0) >= lost.chance {
            return;
        }
        let d = segment.direction();
        let main = (0..3)
            .max_by(|a, b| d[*a].abs().total_cmp(&d[*b].abs()))
            .unwrap_or(0);
        state.lost[main] -= d[main].signum() * lost.size.0;
    }

    /// Where the switches are, in commanded coordinates, on the axes
    /// chosen; the commanded position on the others.
    fn switches(&self, state: &State, which: [bool; 3]) -> Xyz {
        let t = self.travels();
        let p = self.config.pull_off.0;
        let here = axes(state.rest);
        point([0, 1, 2].map(|i| {
            if !which[i] {
                here[i]
            } else if self.config.home_to_max[i] {
                t[i].max.0 + p - state.lost[i]
            } else {
                t[i].min.0 - p - state.lost[i]
            }
        }))
    }

    /// Pulled off the switches: z alone, or x and y.
    fn pulled_off(&self, state: &State, z: bool) -> Xyz {
        let home = axes(Self::home_corner(
            &self.config.capabilities,
            &self.config.home_to_max,
        ));
        let here = axes(state.rest);
        point([0, 1, 2].map(|i| if (i == 2) == z { home[i] } else { here[i] }))
    }

    fn travels(&self) -> [Travel; 3] {
        let c = &self.config.capabilities;
        [c.x, c.y, c.z]
    }

    /// Starts a motion from rest to `target` at `speed`, from `at`.
    fn begin(&self, state: &mut State, at: Timestamp, target: Xyz, speed: f64, purpose: Purpose) {
        let segment = Segment::new(state.rest, target, at, speed, self.config.acceleration);
        Self::archive(state, at);
        state.settle = None;
        state.status = match purpose {
            Purpose::Home(..) => StageStatus::Homing,
            Purpose::Hold(..) => StageStatus::Hold,
            _ => StageStatus::Moving,
        };
        state.hard = self.hard_limit(state, &segment);
        state.motion = Some((segment, purpose));
        if !matches!(purpose, Purpose::Home(..)) {
            self.fire(state, &segment);
        }
    }

    /// When `segment` would bring an axis's true position onto a limit
    /// switch, if it does.
    fn hard_limit(&self, state: &State, segment: &Segment) -> Option<Timestamp> {
        let (half, p, t) = (
            self.config.backlash.0 / 2.0,
            self.config.pull_off.0,
            self.travels(),
        );
        let (from, d) = (axes(segment.from()), segment.direction());
        (0..3)
            .filter_map(|i| {
                let lost = state.lost[i];
                // The commanded position at which the true one meets the
                // switch ahead.
                let x = if d[i] > 0.0 {
                    t[i].max.0 + p - lost + half
                } else if d[i] < 0.0 {
                    t[i].min.0 - p - lost - half
                } else {
                    return None;
                };
                let s = (x - from[i]) / d[i];
                (s >= 0.0 && s <= segment.distance()).then(|| segment.time_at(s))
            })
            .min()
    }

    /// Schedules the pulses that `segment` passes.
    fn fire(&self, state: &mut State, segment: &Segment) {
        let Some((axis, positions)) = state.armed.take() else {
            return;
        };
        let i = index(axis);
        let (from, d) = (axes(segment.from())[i], segment.direction()[i]);
        let mut left = Vec::new();
        for p in positions {
            let s = if d == 0.0 { -1.0 } else { (p - from) / d };
            if s >= 0.0 && s <= segment.distance() {
                state.pulses.push(Pulse {
                    time: segment.time_at(s),
                    axis,
                    position: Um(p),
                });
            } else {
                left.push(p);
            }
        }
        state.armed = Some((axis, left));
    }

    /// Takes back the pulses scheduled after `at`, when the motion stops
    /// early, and arms them again.
    fn unfire(&self, state: &mut State, at: Timestamp) {
        let (late, kept): (Vec<Pulse>, Vec<Pulse>) =
            state.pulses.drain(..).partition(|p| p.time > at);
        state.pulses = kept;
        if let Some(first) = late.first() {
            let axis = first.axis;
            let mut positions: Vec<f64> = late.iter().map(|p| p.position.0).collect();
            if let Some((a, more)) = state.armed.take().filter(|(a, _)| *a == axis) {
                let _ = a;
                positions.extend(more);
            }
            state.armed = Some((axis, positions));
        }
    }

    /// Slows the current motion to a stop from now, for `purpose`.
    fn slow(&self, state: &mut State, now: Timestamp, purpose: Purpose) {
        let Some((segment, _)) = state.motion else {
            return;
        };
        let (here, speed) = (segment.position(now), segment.speed(now));
        self.rest_at(state, now, here);
        self.unfire(state, now);
        let stop = Segment::stop(
            here,
            segment.direction(),
            now,
            speed,
            self.config.acceleration,
        );
        state.hard = self.hard_limit(state, &stop);
        state.motion = Some((stop, purpose));
        state.status = if matches!(purpose, Purpose::Hold(..)) {
            StageStatus::Hold
        } else {
            StageStatus::Moving
        };
    }

    /// Fails unless it can take a command that moves it.
    fn ready(state: &State) -> Result<()> {
        match &state.status {
            StageStatus::Alarm { reason } => Err(Error::Alarm {
                reason: reason.clone(),
            }),
            StageStatus::Idle => Ok(()),
            other => Err(Error::Busy {
                reason: format!("it is {other:?}"),
            }),
        }
    }

    /// Waits until move `id` ends, and gives the first report after it.
    async fn outcome(&self, id: u64) -> Result<PositionSample> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let now = self.clock.now();
            self.pump(now);
            let wake = {
                let state = self.state();
                match state.outcomes.get(&id) {
                    Some(Outcome::Done(at)) => {
                        // The first report taken after the end, however late
                        // this wakes to see it.
                        let first = Timestamp(at.0.div_ceil(self.interval()) * self.interval());
                        if let Some(latest) = self.track.latest().filter(|s| s.time >= first) {
                            // Unless the ring has since dropped it.
                            let sample =
                                self.track
                                    .at(first)
                                    .map_or(latest, |position| PositionSample {
                                        time: first,
                                        position,
                                    });
                            return Ok(sample);
                        }
                    }
                    Some(Outcome::Stopped) => return Err(Error::Stopped),
                    Some(Outcome::Alarm(reason)) => {
                        return Err(Error::Alarm {
                            reason: reason.clone(),
                        });
                    }
                    None => {}
                }
                // The next report's arrival, taken or yet to be, or the
                // motion's end.
                let taken = Timestamp(state.next_report * self.interval());
                let report = state.in_flight.front().map_or(taken, |s| s.time) + self.latency();
                match &state.motion {
                    Some((segment, _)) => report.min(segment.end().max(now)),
                    None => report,
                }
            };
            tokio::select! {
                () = self.clock.sleep_until(wake) => {}
                () = &mut changed => {}
            }
        }
    }

    /// Pauses a move with a feed hold: it slows to a stop, and waits for
    /// [`SimStage::resume`] or a stop.
    pub async fn hold(&self) -> Result<()> {
        let now = self.clock.now();
        self.pump(now);
        let end = {
            let mut state = self.state();
            let Some((_, Purpose::Move(id, target, speed))) = state.motion else {
                return Err(Error::Busy {
                    reason: "only a move can be held".to_owned(),
                });
            };
            self.slow(&mut state, now, Purpose::Hold(id, target, speed));
            state.motion.map(|(s, _)| s.end()).unwrap_or(now)
        };
        self.changed.notify_waiters();
        self.clock.sleep_until(end).await;
        self.pump(self.clock.now());
        Ok(())
    }

    /// Resumes a held move towards its target, at its speed.
    pub async fn resume(&self) -> Result<()> {
        let now = self.clock.now();
        self.pump(now);
        let mut state = self.state();
        let held = match (&state.status, state.motion) {
            (StageStatus::Hold, None) => state.held.take(),
            _ => None,
        };
        let (id, target, speed) = held.ok_or_else(|| Error::Busy {
            reason: "nothing is held".to_owned(),
        })?;
        self.begin(
            &mut state,
            now,
            target,
            speed,
            Purpose::Move(id, target, speed),
        );
        drop(state);
        self.changed.notify_waiters();
        Ok(())
    }

    /// Delivers its reports as they arrive, for as long as it runs. A live
    /// simulator spawns it (P16); tests pump the stage instead, as every
    /// operation does.
    pub async fn run(&self) {
        loop {
            let next = {
                let state = self.state();
                let taken = Timestamp(state.next_report * self.interval());
                state.in_flight.front().map_or(taken, |s| s.time) + self.latency()
            };
            self.clock.sleep_until(next).await;
            self.pump(self.clock.now());
        }
    }
}

impl Stage for SimStage {
    fn capabilities(&self) -> &StageCapabilities {
        &self.config.capabilities
    }

    fn status(&self) -> HalFuture<'_, StageStatus> {
        Box::pin(async move {
            self.pump(self.clock.now());
            Ok(self.state().status.clone())
        })
    }

    fn position(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.pump(self.clock.now());
            self.track.latest().ok_or(Error::Disconnected)
        })
    }

    fn track(&self) -> Arc<PositionTrack> {
        Arc::clone(&self.track)
    }

    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            let caps = &self.config.capabilities;
            caps.check_target(target)?;
            let speed = caps.check_speed(speed)?.0;
            let id = {
                let now = self.clock.now();
                self.pump(now);
                let mut state = self.state();
                Self::ready(&state)?;
                state.moves += 1;
                let id = state.moves;
                self.begin(
                    &mut state,
                    now,
                    target,
                    speed,
                    Purpose::Move(id, target, speed),
                );
                id
            };
            self.changed.notify_waiters();
            self.outcome(id).await
        })
    }

    fn jog(&self, axis: Axis, towards_max: bool, speed: Option<UmPerS>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            let speed = self.config.capabilities.check_speed(speed)?.0;
            let now = self.clock.now();
            self.pump(now);
            let mut state = self.state();
            Self::ready(&state)?;
            let travel = self.travels()[index(axis)];
            let mut target = axes(state.rest);
            target[index(axis)] = if towards_max {
                travel.max.0
            } else {
                travel.min.0
            };
            self.begin(&mut state, now, point(target), speed, Purpose::Jog);
            drop(state);
            self.changed.notify_waiters();
            Ok(())
        })
    }

    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            let now = self.clock.now();
            self.pump(now);
            let end = {
                let mut state = self.state();
                if let Some((id, ..)) = state.held.take() {
                    state.outcomes.insert(id, Outcome::Stopped);
                    state.status = StageStatus::Idle;
                }
                match state.motion {
                    None => None,
                    Some((_, purpose)) => {
                        if let Purpose::Move(id, ..)
                        | Purpose::Hold(id, ..)
                        | Purpose::Home(id, _) = purpose
                        {
                            state.outcomes.insert(id, Outcome::Stopped);
                        }
                        self.slow(&mut state, now, Purpose::Stop);
                        state.motion.map(|(s, _)| s.end())
                    }
                }
            };
            self.changed.notify_waiters();
            if let Some(end) = end {
                self.clock.sleep_until(end).await;
                self.pump(self.clock.now());
            }
            Ok(())
        })
    }

    fn home(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            if !self.config.capabilities.homing {
                return Err(Error::Unsupported {
                    what: "homing".to_owned(),
                });
            }
            let id = {
                let now = self.clock.now();
                self.pump(now);
                let mut state = self.state();
                if !matches!(state.status, StageStatus::Idle | StageStatus::Alarm { .. }) {
                    return Err(Error::Busy {
                        reason: format!("it is {:?}", state.status),
                    });
                }
                state.moves += 1;
                let id = state.moves;
                // Homing finds the switches wherever the stage truly is, so
                // it starts from the true position, with nothing lost.
                let truth = self.truth(&state, now);
                Self::archive(&mut state, now);
                state.rest = truth;
                state.play = axes(truth);
                state.lost = [0.0; 3];
                state.held = None;
                let target = self.switches(&state, [false, false, true]);
                let seek = self.config.seek_speed.0;
                self.begin(&mut state, now, target, seek, Purpose::Home(id, 0));
                id
            };
            self.changed.notify_waiters();
            self.outcome(id).await
        })
    }

    fn clear_alarm(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.pump(self.clock.now());
            let mut state = self.state();
            if matches!(state.status, StageStatus::Alarm { .. }) {
                state.status = StageStatus::Idle;
            }
            Ok(())
        })
    }

    fn arm_trigger(&self, axis: Axis, positions: Vec<Um>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            if !self.config.capabilities.trigger_output {
                return Err(Error::Unsupported {
                    what: "a trigger output".to_owned(),
                });
            }
            self.state().armed = Some((axis, positions.iter().map(|p| p.0).collect()));
            Ok(())
        })
    }
}
