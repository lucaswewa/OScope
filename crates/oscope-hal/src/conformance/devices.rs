//! The conformance suites of the stage, the lights and the objective mount.

use std::collections::HashSet;

use oscope_core::{LightState, Um, Xyz};

use super::{Failure, ensure, expect, refused};
use crate::{
    Axis, Error, Illumination, ObjectiveMount, Stage, StageCapabilities, StageStatus, UmPerS,
};

/// Fails `check` unless `got` is within `tolerance` of `wanted` on each axis.
fn near(got: Xyz, wanted: Xyz, tolerance: Um, check: &'static str) -> Result<(), Failure> {
    let close = [(got.x, wanted.x), (got.y, wanted.y), (got.z, wanted.z)]
        .iter()
        .all(|(g, w)| (g.0 - w.0).abs() <= tolerance.0);
    ensure(close, check, || format!("at {got:?}, not {wanted:?}"))
}

/// Fails `check` unless `position` is within the soft limits, give or take
/// `tolerance`.
fn within(
    caps: &StageCapabilities,
    position: Xyz,
    tolerance: Um,
    check: &'static str,
) -> Result<(), Failure> {
    let inside = [
        (Axis::X, position.x),
        (Axis::Y, position.y),
        (Axis::Z, position.z),
    ]
    .iter()
    .all(|(axis, v)| {
        let t = caps.travel(*axis);
        v.0 >= t.min.0 - tolerance.0 && v.0 <= t.max.0 + tolerance.0
    });
    ensure(inside, check, || {
        format!("{position:?} is outside the soft limits")
    })
}

async fn idle(stage: &dyn Stage, check: &'static str) -> Result<(), Failure> {
    let status = expect(stage.status().await, check)?;
    ensure(status == StageStatus::Idle, check, || {
        format!("it is {status:?}")
    })
}

/// Checks that `stage` does what the [`Stage`] trait promises: that it
/// reaches targets within `tolerance`, refuses what it should, and reports
/// where it is.
pub async fn stage(stage: &dyn Stage, tolerance: Um) -> Result<(), Failure> {
    let caps = stage.capabilities();
    let travel = [caps.x, caps.y, caps.z];
    let sane = travel.iter().all(|t| t.min < t.max)
        && caps.default_speed.0 > 0.0
        && caps.default_speed <= caps.max_speed
        && caps.report_interval.0 > 0.0;
    ensure(sane, "capabilities that make sense", || format!("{caps:?}"))?;

    if let StageStatus::Alarm { .. } = expect(stage.status().await, "read the status")? {
        expect(stage.clear_alarm().await, "clear an alarm")?;
    }
    if caps.homing {
        let homed = expect(stage.home().await, "home")?;
        within(caps, homed.position, tolerance, "home")?;
        idle(stage, "home").await?;
    } else {
        refused(stage.home().await, "refuse to home", |e| {
            matches!(e, Error::Unsupported { .. })
        })?;
    }
    let start = expect(stage.position().await, "read the position")?.position;

    let at = |f: f64| {
        let along = |t: crate::Travel| t.min + (t.max - t.min) * f;
        Xyz::new(along(caps.x), along(caps.y), along(caps.z))
    };
    let track = stage.track();
    for (target, speed) in [(at(0.5), None), (at(0.25), Some(caps.max_speed))] {
        let check = "move to a target";
        let reached = expect(stage.move_to(target, speed).await, check)?;
        near(reached.position, target, tolerance, check)?;
        idle(stage, "be idle after a move").await?;
        let now = expect(stage.position().await, "report where it is")?;
        near(now.position, target, tolerance, "report where it is")?;
        let latest = track.latest();
        let tracked =
            latest.is_some_and(|l| track.at(l.time) == Some(l.position) && l.time >= reached.time);
        ensure(tracked, "track its reports", || {
            format!("its latest sample is {latest:?}")
        })?;
    }
    let delta = at(0.1) - at(0.0);
    let reached = expect(stage.move_by(delta, None).await, "move by a distance")?;
    near(
        reached.position,
        at(0.25) + delta,
        tolerance,
        "move by a distance",
    )?;

    let here = expect(stage.position().await, "read the position")?.position;
    let beyond = Xyz::new(caps.x.max + Um(1_000.0), here.y, here.z);
    refused(
        stage.move_to(beyond, None).await,
        "refuse a target beyond the soft limits",
        |e| matches!(e, Error::OutOfRange { .. }),
    )?;
    let fast = Some(UmPerS(caps.max_speed.0 * 2.0));
    refused(
        stage.move_to(at(0.5), fast).await,
        "refuse a speed over the fastest",
        |e| matches!(e, Error::OutOfRange { .. }),
    )?;
    refused(
        stage.move_to(at(0.5), Some(UmPerS(0.0))).await,
        "refuse a speed of 0",
        |e| matches!(e, Error::Unsupported { .. }),
    )?;
    let after = expect(stage.position().await, "stay put when refusing")?.position;
    near(after, here, tolerance, "stay put when refusing")?;

    expect(stage.jog(Axis::X, true, None).await, "jog")?;
    expect(stage.stop().await, "stop a jog")?;
    idle(stage, "be idle after a stop").await?;
    let stopped = expect(stage.position().await, "stop a jog")?.position;
    within(caps, stopped, tolerance, "stop a jog")?;

    expect(
        stage.move_to(start, None).await,
        "go back to where it started",
    )?;
    Ok(())
}

/// Checks that `lights` do what the [`Illumination`] trait promises.
pub async fn illumination(lights: &dyn Illumination) -> Result<(), Failure> {
    let caps = lights.capabilities();
    let names: Vec<&str> = caps.channels.iter().map(|c| c.name.as_str()).collect();
    let unique = names.iter().collect::<HashSet<_>>().len() == names.len();
    ensure(
        !names.is_empty() && unique,
        "capabilities that make sense",
        || format!("channels {names:?}"),
    )?;
    let original = expect(lights.state().await, "read the state")?;
    let order: Vec<&str> = original.iter().map(|l| l.channel.as_str()).collect();
    ensure(order == names, "a state for each light, in order", || {
        format!("{order:?}")
    })?;

    for (i, name) in names.iter().enumerate() {
        let check = "set each light";
        let asked = LightState {
            channel: (*name).to_owned(),
            on: true,
            intensity: 0.5,
        };
        let applied = expect(lights.set(asked.clone()).await, check)?;
        let expected = expect(caps.check(&asked), check)?;
        ensure(applied == expected, check, || {
            format!("{asked:?} applied as {applied:?}")
        })?;
        let state = expect(lights.state().await, check)?;
        ensure(state.get(i) == Some(&applied), "keep what was set", || {
            format!("{state:?}")
        })?;
    }

    let before = expect(lights.state().await, "read the state")?;
    let bright = LightState {
        channel: names[0].to_owned(),
        on: true,
        intensity: 2.0,
    };
    refused(
        lights.set(bright).await,
        "refuse an intensity over 1",
        |e| matches!(e, Error::OutOfRange { .. }),
    )?;
    let unknown = LightState {
        channel: "no such light".to_owned(),
        on: true,
        intensity: 0.5,
    };
    refused(lights.set(unknown).await, "refuse an unknown light", |e| {
        matches!(e, Error::Unknown { .. })
    })?;
    let after = expect(lights.state().await, "read the state")?;
    ensure(after == before, "change nothing when refusing", || {
        format!("{before:?} became {after:?}")
    })?;

    for light in original {
        expect(lights.set(light).await, "put the lights back")?;
    }
    Ok(())
}

/// Checks that `mount` does what the [`ObjectiveMount`] trait promises.
pub async fn objective_mount(mount: &dyn ObjectiveMount) -> Result<(), Failure> {
    let catalogue = mount.catalogue();
    expect(catalogue.validate(), "a valid catalogue")?;
    let original = expect(mount.fitted().await, "read the objective fitted")?;
    for objective in &catalogue.objectives {
        let check = "fit each objective";
        let fitted = expect(mount.fit(objective.name.clone()).await, check)?;
        let read = expect(mount.fitted().await, check)?;
        ensure(
            fitted == *objective && read.as_ref() == Some(objective),
            check,
            || format!("fitted {fitted:?}, read {read:?}"),
        )?;
    }
    let before = expect(mount.fitted().await, "read the objective fitted")?;
    refused(
        mount.fit("no such objective".into()).await,
        "refuse an unknown objective",
        |e| matches!(e, Error::Unknown { .. }),
    )?;
    let after = expect(mount.fitted().await, "read the objective fitted")?;
    ensure(after == before, "change nothing when refusing", || {
        format!("{before:?} became {after:?}")
    })?;
    if let Some(objective) = original {
        expect(mount.fit(objective.name).await, "put the objective back")?;
    }
    Ok(())
}
