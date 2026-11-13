//! Motion along a straight path, timed by the simulation clock, with
//! grblHAL's trapezoidal acceleration (D30, ADR-0013).

use std::time::Duration;

use oscope_core::{Timestamp, Um, Xyz};

/// A straight move from a point, accelerating from a starting speed to a
/// cruising speed, then decelerating to rest at the end.
///
/// - **Without acceleration,** it moves at its cruising speed throughout:
///   a distance d at speed v takes d ÷ v.
/// - **With acceleration a,** a move long enough to reach v takes
///   d ÷ v + v ÷ a; a shorter one accelerates to its middle and back down,
///   in 2 √(d ÷ a).
/// - **A multi-axis move** follows the straight path at the set speed, as
///   grblHAL's feed rate does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    from: [f64; 3],
    direction: [f64; 3],
    distance: f64,
    start: Timestamp,
    start_speed: f64,
    peak: f64,
    accel: f64,
    decel: f64,
    /// The durations of the three phases, in seconds.
    phases: [f64; 3],
}

fn axes(p: Xyz) -> [f64; 3] {
    [p.x.0, p.y.0, p.z.0]
}

fn point(a: [f64; 3]) -> Xyz {
    Xyz::new(Um(a[0]), Um(a[1]), Um(a[2]))
}

impl Segment {
    /// From `from` to `to`, starting from rest at `start`, cruising at
    /// `speed` µm/s, with acceleration `accel` µm/s², or none.
    pub fn new(from: Xyz, to: Xyz, start: Timestamp, speed: f64, accel: Option<f64>) -> Self {
        let delta = [to.x.0 - from.x.0, to.y.0 - from.y.0, to.z.0 - from.z.0];
        let distance = delta.iter().map(|d| d * d).sum::<f64>().sqrt();
        let direction = if distance > 0.0 {
            delta.map(|d| d / distance)
        } else {
            [0.0; 3]
        };
        Self::along(axes(from), direction, distance, start, 0.0, speed, accel)
    }

    /// A stop: from `from`, moving along the unit vector `direction` at
    /// `speed`, decelerating at `accel` to rest; at once without it.
    pub fn stop(
        from: Xyz,
        direction: [f64; 3],
        start: Timestamp,
        speed: f64,
        accel: Option<f64>,
    ) -> Self {
        match accel {
            Some(a) if a > 0.0 && speed > 0.0 => {
                let distance = speed * speed / (2.0 * a);
                Self::along(axes(from), direction, distance, start, speed, speed, accel)
            }
            _ => Self::new(from, from, start, 1.0, None),
        }
    }

    fn along(
        from: [f64; 3],
        direction: [f64; 3],
        distance: f64,
        start: Timestamp,
        start_speed: f64,
        speed: f64,
        accel: Option<f64>,
    ) -> Self {
        let mut s = Self {
            from,
            direction,
            distance,
            start,
            start_speed: 0.0,
            peak: speed.max(f64::MIN_POSITIVE),
            accel: f64::INFINITY,
            decel: f64::INFINITY,
            phases: [0.0, distance / speed.max(f64::MIN_POSITIVE), 0.0],
        };
        let Some(a) = accel.filter(|a| *a > 0.0) else {
            return s;
        };
        if distance <= 0.0 {
            s.phases = [0.0; 3];
            return s;
        }
        let v0 = start_speed.min(speed).max(0.0);
        // The peak that accelerating from v0 and decelerating to rest reach
        // over the distance, capped at the cruising speed.
        let peak = (a * distance + v0 * v0 / 2.0).sqrt().min(speed).max(v0);
        // A start too fast to stop in the distance decelerates harder.
        let decel = if v0 * v0 > 2.0 * a * distance {
            v0 * v0 / (2.0 * distance)
        } else {
            a
        };
        let (d1, d3) = (
            (peak * peak - v0 * v0) / (2.0 * a),
            peak * peak / (2.0 * decel),
        );
        let d2 = (distance - d1 - d3).max(0.0);
        s.start_speed = v0;
        s.peak = peak;
        s.accel = a;
        s.decel = decel;
        s.phases = [(peak - v0) / a, d2 / peak, peak / decel];
        s
    }

    /// Where it starts.
    pub fn from(&self) -> Xyz {
        point(self.from)
    }

    /// Where it ends.
    pub fn to(&self) -> Xyz {
        point(self.at_distance(self.distance))
    }

    /// The unit vector along it; zero if it doesn't move.
    pub fn direction(&self) -> [f64; 3] {
        self.direction
    }

    /// Its length, in µm.
    pub fn distance(&self) -> f64 {
        self.distance
    }

    /// When it starts.
    pub fn start(&self) -> Timestamp {
        self.start
    }

    /// How long it takes.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.phases.iter().sum())
    }

    /// When it ends.
    pub fn end(&self) -> Timestamp {
        self.start + self.duration()
    }

    fn seconds(&self, at: Timestamp) -> f64 {
        at.0.saturating_sub(self.start.0) as f64 / 1e9
    }

    /// How far along it has gone at `at`: 0 before it starts, all of it
    /// after it ends.
    pub fn travelled(&self, at: Timestamp) -> f64 {
        if at >= self.end() {
            return self.distance;
        }
        let t = self.seconds(at);
        let [t1, t2, t3] = self.phases;
        let (v0, vp) = (self.start_speed, self.peak);
        let d1 = if t1 > 0.0 {
            v0 * t1 + 0.5 * self.accel * t1 * t1
        } else {
            0.0
        };
        let d2 = vp * t2;
        let s = if t < t1 {
            v0 * t + 0.5 * self.accel * t * t
        } else if t < t1 + t2 {
            d1 + vp * (t - t1)
        } else if t < t1 + t2 + t3 {
            let u = t - t1 - t2;
            d1 + d2 + vp * u - 0.5 * self.decel * u * u
        } else {
            self.distance
        };
        s.clamp(0.0, self.distance)
    }

    /// Its speed at `at`, in µm/s.
    pub fn speed(&self, at: Timestamp) -> f64 {
        let t = self.seconds(at);
        let [t1, t2, t3] = self.phases;
        if at < self.start || t >= t1 + t2 + t3 {
            0.0
        } else if t < t1 {
            self.start_speed + self.accel * t
        } else if t < t1 + t2 {
            self.peak
        } else {
            (self.peak - self.decel * (t - t1 - t2)).max(0.0)
        }
    }

    fn at_distance(&self, s: f64) -> [f64; 3] {
        [0, 1, 2].map(|i| self.from[i] + self.direction[i] * s)
    }

    /// Where it is at `at`.
    pub fn position(&self, at: Timestamp) -> Xyz {
        point(self.at_distance(self.travelled(at)))
    }

    /// When it has gone `s` along it, the inverse of
    /// [`Segment::travelled`].
    pub fn time_at(&self, s: f64) -> Timestamp {
        let [t1, t2, t3] = self.phases;
        let (v0, vp, a, b) = (self.start_speed, self.peak, self.accel, self.decel);
        let d1 = if t1 > 0.0 {
            v0 * t1 + 0.5 * a * t1 * t1
        } else {
            0.0
        };
        let d2 = vp * t2;
        let t = if s <= 0.0 {
            0.0
        } else if s < d1 {
            (-v0 + (v0 * v0 + 2.0 * a * s).sqrt()) / a
        } else if s < d1 + d2 {
            t1 + (s - d1) / vp
        } else if s < self.distance && t3 > 0.0 {
            let r = s - d1 - d2;
            t1 + t2 + (vp - (vp * vp - 2.0 * b * r).max(0.0).sqrt()) / b
        } else {
            t1 + t2 + t3
        };
        self.start + Duration::from_secs_f64(t)
    }
}
