# P11: Simulation clock and grblHAL-like stage

- Status: In review
- Branch / PR: `m1/p11-sim-stage` / [#23](https://github.com/lucaswewa/OScope/pull/23)
- Size: L · estimated ~2,100 before it started, from P10's measured densities; the owner chose one pull request · measured **2,503** hand-written lines added (code 1,363; tests 688; examples 446; glue 6), and 2 removed. Docs and `Cargo.lock` aren't counted. **That is 1,603 over L's 900, and 403 over the estimate,** most of it in the stage itself (*Design notes*).

## Summary

The simulator now has its clock, and a stage that moves like grblHAL on it ([spec](../phases.md#p11), ADR-0013 §7, D20, D30).

- **`SimClock`:**
  - real time (1×) by default, scaled, or stepped by tests;
  - it implements `Clock`, so it can be the server's clock whenever the simulator runs;
  - it never moves when its rate changes;
  - `sleep_until` waits on simulated time at any rate.
- **`Segment`:** a straight move with grblHAL's trapezoidal acceleration.
  - A distance d at speed v takes d ÷ v, plus v ÷ a with acceleration a, or 2 √(d ÷ a) when it is too short to reach v.
  - A multi-axis move keeps its speed along the path.
- **`SimStage`:**
  - moves, jogs, stops, feed holds and resumes, and homing on its switches, all timed by the clock;
  - soft limits refuse a target, and a limit switch stops it with an alarm;
  - its true position differs from the commanded one by backlash, lost steps and settling, and its encoders report the true one;
  - status reports every 10 ms, arriving 5 ms later, feed its `PositionTrack`;
  - its trigger output pulses at armed positions.

  Its state is a function of the clock, however often it is read. It passes the HAL's conformance suite.
- **`Stage::arm_trigger`** joins the HAL, as ADR-0012 planned: optional, behind `trigger_output`. The fake stage implements it, and the suite checks it.
- **`ex-sim-stage`** homes, moves and jogs at 10×, printing each motion's planned and measured duration, and serves a page plotting position against time.

## What was built

| Where | What |
|---|---|
| `oscope_sim::clock` | **`Rate`** (`RealTime`, `Scaled(k)`, `Stepped`) and **`SimClock`**: `new`, `rate`, `set_rate`, `advance`, `advance_to`, `sleep_until`, `sleep`; it implements `Clock` |
| `oscope_sim::motion` | **`Segment`:** `new(from, to, start, speed, accel)`, `stop(from, direction, start, speed, accel)`, `from`, `to`, `direction`, `distance`, `start`, `duration`, `end`, `travelled`, `speed`, `position`, `time_at` |
| `oscope_sim::stage` | **`SimStageConfig`** (capabilities, acceleration, seek speed, pull-off, homing directions, latency, backlash, `LostSteps`, `Settling`, the true start, whether homing is required, a seed), with grblHAL-like defaults. **`SimStage`:** the `Stage` trait, and `pump`, `run`, `true_position`, `pulses`, `hold`, `resume`. **`Pulse`** |
| `oscope_hal::stage` | **`Stage::arm_trigger(axis, positions)`,** unsupported by default |
| `oscope_hal::fake` | **`FakeStage::arm_trigger`** and **`armed`** |
| `oscope_hal::conformance` | The stage's suite arms the trigger output it has, refuses one it lacks, and goes back to where it started within its soft limits |
| `examples/ex-sim-stage` | Homing, four moves, trigger pulses and a jog at 10×; the motions planned against measured; the reports plotted on a page; `--rate`, `--smoke` |
| Docs | P10 marked *Done*. The change log records P11's estimate and the owner's choice |

**The stage's defaults:**

| Setting | Value |
|---|---|
| Travel (soft limits) | x 0–100 mm, y 0–75 mm, z 0–20 mm |
| Speeds | 10 mm/s by default, 25 mm/s at most, along the path |
| Acceleration | 100 mm/s² |
| Homing | at 5 mm/s: z up to its switch, then x and y down to theirs; each switch 1 mm beyond its soft limit, pulled off to the soft limit |
| Reports | every 10 ms, arriving 5 ms later |
| Backlash | 2 µm |
| Settling | a 0.3 µm overshoot, falling to 1/e in 15 ms |
| Lost steps | none, unless set |
| At switch-on | truly at (50, 37.5, 10) mm, believing itself at its home corner, in alarm until homed |

## How to try it

```sh
cargo run --release -p ex-sim-stage
cargo test -p oscope-sim
```

The example's [README](../../../examples/ex-sim-stage/README.md) shows what it prints. Each motion ends within 10 ms of its plan: a move answers with the first report taken after it ends, and reports come every 10 ms.

## Performance

The stage's work is a few segment evaluations for each report, too little to measure. **The example simulates 23.45 s in 2.34 s at 10×,** and 24.4 s in 1.22 s at 20× in a debug build.

**What a running clock does to the host's timing:** a sleep on the host is late by its timer's resolution. At k×, simulated time runs on by k times that. So a command the host sends after a sleep (the example's jog, stopped after "0.5 s") lands later in simulated time at high rates: 0.3 s later at 10×. Motions don't drift, being functions of the clock; only commands timed by the host do. Tests that need exact timing use the stepped clock.

## Hardware

None was used. The stage's defaults stand in for the controller's `$$`, which H0 records (P02b).

## Design notes and deviations from the plan

- **One pull request, though it is nearly three times L,** at the owner's choice. It came in 403 lines over the estimate. The stage alone is 898 lines against ~600: hard limits, homing's four legs, holds, pulses taken back on a stop, and reports in time order were each more than budgeted.
- **Soft limits refuse rather than alarm.**
  - grblHAL raises an alarm when a G-code move would cross a soft limit. The HAL's conformance suite wants such a target refused, with the stage unmoved, and M3's driver will check targets with the capabilities before sending them.
  - So the simulated stage refuses them. Hard limits, a switch 1 mm beyond each soft limit, raise the alarm.
  - A jog stops at the soft limit, as grblHAL's jogs do.
- **Hard limits are reached as on the real machine.** Until it homes, the stage believes it is at its home corner, wherever it truly is. A move can then drive it onto a switch, and so can lost steps.
- **One speed and one acceleration for the path,** not a rate and acceleration for each axis as grblHAL has. The axes' own limits are for H0, if the instrument needs them.
- **A feed hold and its resume** are the simulated stage's own methods, not the HAL's. The trait's `stop` ends a held move.
- **The conformance suite changed, for a real reason.** After homing, the encoder reads x = −1 µm, half the backlash past the soft limit, as a real encoder would. The suite then tried to go back there, and was rightly refused. It now goes back to that position within the soft limits.
- **The stage's tests and example found four bugs, all fixed:**
  - **A hang.** `pump` announced every change, and a waiting move listened for changes and pumped. So it woke itself forever without yielding, and on a single-threaded runtime nothing else ran. `pump` now announces nothing; commands do.
  - **A rounded end.** A move's end is a whole nanosecond, so its position there fell up to 10⁻⁵ µm short of the target, which the backlash's exact test saw. A segment is now at its end from its end timestamp on.
  - **A report at an event's instant** was taken before the event, so a move's first report after its end could miss its lost steps. It is now taken after.
  - **A late answer.** A move answered with the latest report, so a caller that woke late got a later one. At 200× in a debug build, the example's moves came out seconds late. A move now answers with the first report taken after it ends. A test jumps the clock 5 s past a 1 s move, and checks that the answer is the report from 1 s.
- **The track's timestamps** are the moments the reports were taken, as if the driver subtracted the latency exactly (ADR-0007). A positive estimated-latency error is for H0 to measure.
- **Settling** is an overshoot that dies away exponentially, without ringing. It is enough for acquisition to wait on (P16).

## Tests

**71 tests across the two crates.**

`oscope-sim` (41, 26 of them new):

| File | Tests |
|---|---|
| `tests/clock.rs` (5) | A stepped clock moves only when advanced, and never back. A sleeper wakes when the clock passes its moment, not before. Scaled 20×, 2 s takes 100 ms on the host. 1× follows the host. Changing the rate never moves the clock, and a stepped sleeper wakes once the clock runs |
| `tests/motion.rs` (5) | d ÷ v without acceleration. d ÷ v + v ÷ a, or 2 √(d ÷ a), with it, over 1,000 random moves, to 1 ns. A multi-axis move keeps its path speed and line. Time and distance are each other's inverse. A stop takes v ÷ a over v² ÷ 2a |
| `tests/stage.rs` (14) | On a stepped clock: <ul><li>a move takes its planned time, with and without acceleration;</li><li>it answers with the first report after its end, however late it is looked at;</li><li>multi-axis path speed;</li><li>soft limits refuse, and jogs stop at them;</li><li>a hard limit's alarm, until homing;</li><li>homing's four legs, timed;</li><li>a stop's v² ÷ 2a, and a hold until resume;</li><li>backlash's half-width lag while moving, and its dead band as a property test over 60 random moves;</li><li>lost steps with and without encoders;</li><li>settling's decay;</li><li>reports every interval after the latency;</li><li>trigger pulses at their positions, taken back by a stop and fired later.</li></ul>Then the HAL's conformance suite at 500× |
| `tests/realtime.rs` (2) | A 0.4 s move takes 0.4–0.515 s on the host at 1×, and 40–102 ms at 10× |

`oscope-hal` (30): the conformance suite now checks the trigger output on the fakes.

**Breaking the code on purpose.** 15 changes, each made alone, and each caught:

| Change | Caught by |
|---|---|
| A scaled clock at 1× | `scaled_time_runs_faster_than_the_host` |
| A change of rate that moves the clock | `a_change_of_rate_never_moves_the_clock` |
| Ramps that never reach full speed | the motion and path-speed tests |
| A stop over v² ÷ a | `a_stop_slows_down_over_v_squared_over_twice_a` |
| The whole backlash as the lag | `while_moving_the_backlash_lags_by_half` |
| Steps lost ahead of the motion | `lost_steps_show_on_the_encoders_and_not_without_them` |
| Settling that never decays; no settling at all | `the_stage_settles_after_a_move` |
| Reports that arrive at once | `reports_arrive_every_interval_after_the_latency` |
| Hard limits that forget the lost position | `a_hard_limit_raises_an_alarm_until_the_stage_is_homed` |
| z homing down | the hard-limit and homing tests |
| Pulses half-way to their positions | `the_trigger_output_pulses_at_its_positions` |
| A stop that keeps the pulses past it | the same |
| Answering with the latest report | six tests, among them `a_move_answers_with_the_first_report_after_it_ends` |
| A fake stage that won't arm the trigger it has | the HAL's conformance tests |

**The first run missed two,** and the tests were strengthened:

- **The whole backlash as the lag** shows only while moving; at rest the stage carries its backlash state through correctly. So `while_moving_the_backlash_lags_by_half` was added.
- **A stop that kept the pulses past it** went unseen by coincidence: the next move resumed at once, at the same speed, and passed 40 mm at exactly the dropped pulse's time. The trigger test now waits a second first.

**The workspace,** with every step CI runs: `cargo fmt`, `clippy -D warnings`, `cargo test` (171 tests), `cargo doc -D warnings`, `cargo deny`, the server's `--version`, and every example's `--smoke`.

- **On Windows:** see the pull request.
- **Under WSL (Ubuntu 24.04):** see the pull request.

P11 touches no CUDA path.

## Follow-ups and known gaps

- **P12:**
  - the simulated camera renders at `SimStage::true_position` through its exposure;
  - its hardware trigger listens for the stage's `pulses`;
  - frames are timed by the same clock.
- **P16:**
  - the `SimClock` as the server's clock;
  - a `SimStage::run` task for live reports;
  - acquisition's wait for the report after a frame's midpoint.
- **H0:** the real controller's `$$` (travel, rates, accelerations for each axis), its report rate and latency, and how its encoders are read. Then the simulated defaults follow it.
- **`Error::Stopped`,** which P09b's fake couldn't produce, is now checked in the simulated stage's tests.

## Clean room

Written for this phase from plan §5.10, ADR-0013, and grblHAL's documented behaviour (the grblHAL core wiki: status reports, jogging, homing, alarms; `docs/hardware.md` sources 5–7). The motion profile is the textbook trapezoid: constant acceleration, cruise, constant deceleration. The backlash is the play (hysteresis) operator. Nothing was copied from `microscope`, grblHAL's source, or any other codebase.
