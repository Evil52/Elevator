use super::{MotionParams, UM};

#[derive(Clone, Copy, Debug)]
pub struct RestTrip {
    distance_mm: u32,
    duration_ms: u32,
    ramp_ms: u32,
    jerk_ms: u32,
}

impl MotionParams {
    #[must_use]
    pub fn rest_trip(&self, distance_mm: u32) -> Option<RestTrip> {
        let plan = self.stop_plan_um(u128::from(distance_mm) * UM, 0)?;
        let duration_ms = u32::try_from(plan.duration_ms()).ok()?;
        let ramp_ms = u32::try_from(plan.ramps_ms / 2).ok()?;
        let rise_ms = (1000 * u64::from(self.accel_mm_s2)).div_ceil(u64::from(self.jerk_mm_s3));
        let jerk_ms = u32::try_from(rise_ms.min(u64::from(ramp_ms / 2))).ok()?;
        Some(RestTrip {
            distance_mm,
            duration_ms,
            ramp_ms,
            jerk_ms,
        })
    }
}

impl RestTrip {
    #[must_use]
    pub const fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    #[must_use]
    pub const fn ramp_ms(&self) -> u32 {
        self.ramp_ms
    }

    #[must_use]
    pub fn sample(&self, elapsed_ms: u32) -> (u32, u32) {
        if elapsed_ms >= self.duration_ms {
            return (self.distance_mm, 0);
        }
        let t = u128::from(elapsed_ms);
        let total = u128::from(self.duration_ms);
        let ramp = u128::from(self.ramp_ms);
        let rise = u128::from(self.jerk_ms);
        let scale = (6 * rise * (ramp - rise)).max(2);
        let area = scale * (total - ramp);
        let (path, speed) = if t < ramp {
            ramp_sample(t, ramp, rise, scale)
        } else if total - t < ramp {
            let (remaining, velocity) = ramp_sample(total - t, ramp, rise, scale);
            (area - remaining, velocity)
        } else {
            (scale / 2 * (2 * t - ramp), scale)
        };
        let distance = u128::from(self.distance_mm);
        (
            nearest(distance * path, area),
            nearest(1000 * distance * speed, area),
        )
    }
}

fn ramp_sample(t: u128, ramp: u128, rise: u128, scale: u128) -> (u128, u128) {
    if t <= rise {
        (t.pow(3), 3 * t * t)
    } else if ramp - t <= rise {
        let back = ramp - t;
        (
            scale / 2 * (2 * t - ramp) + back.pow(3),
            scale - 3 * back * back,
        )
    } else {
        (
            rise * (3 * t * (t - rise) + rise * rise),
            3 * rise * (2 * t - rise),
        )
    }
}

fn nearest(n: u128, d: u128) -> u32 {
    u32::try_from((n + d / 2) / d).unwrap_or(u32::MAX)
}
