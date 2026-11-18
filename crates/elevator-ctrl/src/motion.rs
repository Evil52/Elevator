const UM: u128 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionParams {
    max_speed_mm_s: u32,
    accel_mm_s2: u32,
    jerk_mm_s3: u32,
}

impl MotionParams {
    #[must_use]
    pub const fn new(max_speed_mm_s: u32, accel_mm_s2: u32, jerk_mm_s3: u32) -> Option<Self> {
        if max_speed_mm_s == 0 || accel_mm_s2 == 0 || jerk_mm_s3 == 0 {
            return None;
        }
        Some(Self {
            max_speed_mm_s,
            accel_mm_s2,
            jerk_mm_s3,
        })
    }

    #[must_use]
    pub const fn max_speed_mm_s(&self) -> u32 {
        self.max_speed_mm_s
    }

    #[must_use]
    pub fn braking_distance_mm(&self, speed_mm_s: u32) -> u32 {
        let (_, micrometres) = self.ramp(0, u128::from(speed_mm_s) * UM);
        saturate(micrometres.div_ceil(UM))
    }

    #[must_use]
    pub fn stopping_distance_mm(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<u32> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let (_, brake_um) = self.ramp(0, settle.speed);
        Some(saturate((settle.path + brake_um).div_ceil(UM)))
    }

    #[must_use]
    pub fn time_to_stop_ms(&self, distance_mm: u32, speed_mm_s: u32) -> Option<u32> {
        self.time_to_stop_from_ms(distance_mm, speed_mm_s, 0)
    }

    #[must_use]
    pub fn time_to_stop_from_ms(
        &self,
        distance_mm: u32,
        speed_mm_s: u32,
        accel_mm_s2: u32,
    ) -> Option<u32> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let rest = (u128::from(distance_mm) * UM).checked_sub(settle.path)?;
        Some(saturate(
            settle.millis + self.stop_time_um(rest, settle.speed)?,
        ))
    }

    fn settle(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<Settled> {
        if speed_mm_s > self.max_speed_mm_s || accel_mm_s2 > self.accel_mm_s2 {
            return None;
        }
        let speed = u128::from(speed_mm_s) * UM;
        let accel = u128::from(accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        let settled = Settled {
            millis: (1000 * accel).div_ceil(jerk),
            path: (3 * speed * accel * jerk + accel * accel * accel).div_ceil(3 * jerk * jerk),
            speed: speed + (accel * accel).div_ceil(2 * jerk),
        };
        (settled.speed <= u128::from(self.max_speed_mm_s) * UM).then_some(settled)
    }

    fn stop_time_um(&self, distance: u128, start: u128) -> Option<u128> {
        if self.ramp(0, start).1 > distance {
            return None;
        }
        if distance == 0 {
            return Some(0);
        }

        let (mut low, mut high) = (start, u128::from(self.max_speed_mm_s) * UM);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if self.profile(start, mid).1 <= distance {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let cruise = low.max(1);
        let (ramps_ms, covered) = self.profile(start, cruise);
        let cruise_ms = (1000 * distance.saturating_sub(covered)).div_ceil(cruise);
        Some(ramps_ms + cruise_ms)
    }

    fn profile(&self, start: u128, cruise: u128) -> (u128, u128) {
        let (accel_ms, accel_um) = self.ramp(start, cruise);
        let (brake_ms, brake_um) = self.ramp(0, cruise);
        (accel_ms + brake_ms, accel_um + brake_um)
    }

    fn ramp(&self, from: u128, to: u128) -> (u128, u128) {
        let delta = to - from;
        let sum = from + to;
        let accel = u128::from(self.accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        if delta * jerk >= accel * accel {
            let numerator = delta * jerk + accel * accel;
            (
                (1000 * numerator).div_ceil(accel * jerk),
                (sum * numerator).div_ceil(2 * accel * jerk),
            )
        } else {
            (
                ceil_sqrt_ratio(4_000_000 * delta, jerk),
                ceil_sqrt_ratio(sum * sum * delta, jerk),
            )
        }
    }
}

struct Settled {
    millis: u128,
    path: u128,
    speed: u128,
}

fn saturate(value: u128) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn ceil_sqrt(n: u128) -> u128 {
    let root = n.isqrt();
    if root * root < n { root + 1 } else { root }
}

fn ceil_sqrt_ratio(num: u128, den: u128) -> u128 {
    ceil_sqrt(num.div_ceil(den))
}
