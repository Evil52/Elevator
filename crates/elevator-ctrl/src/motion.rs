mod moving;
mod trip;
pub use moving::MovingTrip;
pub use trip::RestTrip;

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
        saturate(div_ceil(micrometres, UM))
    }

    #[must_use]
    pub fn stopping_distance_mm(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<u32> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let (_, brake_um) = self.ramp(0, settle.speed);
        Some(saturate(div_ceil(settle.path + brake_um, UM)))
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
        Some(saturate(
            self.moving_plan(distance_mm, speed_mm_s, accel_mm_s2)?
                .duration_ms(),
        ))
    }

    fn moving_plan(
        &self,
        distance_mm: u32,
        speed_mm_s: u32,
        accel_mm_s2: u32,
    ) -> Option<MovingPlan> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let total = u128::from(distance_mm) * UM;
        let rest = total.checked_sub(settle.path)?;
        if accel_mm_s2 == 0 {
            return Some(MovingPlan {
                launch_ms: 0,
                start: settle.speed,
                plan: self.stop_plan_um(rest, settle.speed)?,
            });
        }
        if self.ramp(0, settle.speed).1 > rest {
            return None;
        }
        let speed = u128::from(speed_mm_s) * UM;
        let accel = u128::from(accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        let start = speed.saturating_sub(settle.speed - speed);
        let launch_um = start * accel / jerk + accel * accel * accel / (6 * jerk * jerk);
        Some(MovingPlan {
            launch_ms: 1000 * accel / jerk,
            start,
            plan: self.cruise_plan(total + launch_um, start, settle.speed),
        })
    }

    fn settle(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<Settled> {
        if speed_mm_s > self.max_speed_mm_s || accel_mm_s2 > self.accel_mm_s2 {
            return None;
        }
        let speed = u128::from(speed_mm_s) * UM;
        let accel = u128::from(accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        let settled = Settled {
            path: div_ceil(
                3 * speed * accel * jerk + accel * accel * accel,
                3 * jerk * jerk,
            ),
            speed: speed + div_ceil(accel * accel, 2 * jerk),
        };
        (settled.speed <= u128::from(self.max_speed_mm_s) * UM).then_some(settled)
    }

    fn stop_plan_um(&self, distance: u128, start: u128) -> Option<StopPlan> {
        if self.ramp(0, start).1 > distance {
            return None;
        }
        Some(self.cruise_plan(distance, start, start))
    }

    fn cruise_plan(&self, distance: u128, start: u128, least: u128) -> StopPlan {
        if distance == 0 {
            return StopPlan {
                ramps_ms: 0,
                cruise_ms: 0,
                cruise: start,
            };
        }

        let top = u128::from(self.max_speed_mm_s) * UM;
        let (mut low, mut high) = (least, top.max(least));
        if self.profile_um(start, high) <= distance {
            low = high;
        }
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if self.profile_um(start, mid) <= distance {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let cruise = low.max(1);
        let (ramps_ms, covered) = self.profile(start, cruise);
        let cruise_ms = div_ceil(1000 * distance.saturating_sub(covered), cruise);
        StopPlan {
            ramps_ms,
            cruise_ms,
            cruise,
        }
    }

    fn profile(&self, start: u128, cruise: u128) -> (u128, u128) {
        let (brake_ms, brake_um) = self.ramp(0, cruise);
        if start == 0 {
            return (2 * brake_ms, 2 * brake_um);
        }
        let (accel_ms, accel_um) = self.ramp(start, cruise);
        (accel_ms + brake_ms, accel_um + brake_um)
    }

    fn profile_um(&self, start: u128, cruise: u128) -> u128 {
        let brake_um = self.ramp_um(0, cruise);
        if start == 0 {
            return 2 * brake_um;
        }
        self.ramp_um(start, cruise) + brake_um
    }

    fn ramp_um(&self, from: u128, to: u128) -> u128 {
        let delta = to - from;
        let sum = from + to;
        let accel = u128::from(self.accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        if delta * jerk >= accel * accel {
            let numerator = delta * jerk + accel * accel;
            div_ceil(sum * numerator, 2 * accel * jerk)
        } else {
            ceil_sqrt_ratio(sum * sum * delta, jerk)
        }
    }

    fn ramp(&self, from: u128, to: u128) -> (u128, u128) {
        let delta = to - from;
        let sum = from + to;
        let accel = u128::from(self.accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        if delta * jerk >= accel * accel {
            let numerator = delta * jerk + accel * accel;
            (
                div_ceil(1000 * numerator, accel * jerk),
                div_ceil(sum * numerator, 2 * accel * jerk),
            )
        } else {
            (
                ceil_sqrt_ratio(4_000_000 * delta, jerk),
                ceil_sqrt_ratio(sum * sum * delta, jerk),
            )
        }
    }
}

struct StopPlan {
    ramps_ms: u128,
    cruise_ms: u128,
    cruise: u128,
}

struct MovingPlan {
    launch_ms: u128,
    start: u128,
    plan: StopPlan,
}

impl MovingPlan {
    const fn duration_ms(&self) -> u128 {
        self.plan.duration_ms().saturating_sub(self.launch_ms)
    }
}

impl StopPlan {
    const fn duration_ms(&self) -> u128 {
        self.ramps_ms + self.cruise_ms
    }
}

struct Settled {
    path: u128,
    speed: u128,
}

fn saturate(value: u128) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn div_ceil(num: u128, den: u128) -> u128 {
    match (u64::try_from(num), u64::try_from(den)) {
        (Ok(num), Ok(den)) => u128::from(num.div_ceil(den)),
        _ => num.div_ceil(den),
    }
}

fn ceil_sqrt(n: u128) -> u128 {
    if let Ok(small) = u64::try_from(n) {
        let root = small.isqrt();
        return u128::from(if root * root < small { root + 1 } else { root });
    }
    let root = n.isqrt();
    if root * root < n { root + 1 } else { root }
}

fn ceil_sqrt_ratio(num: u128, den: u128) -> u128 {
    ceil_sqrt(div_ceil(num, den))
}

#[derive(Clone, Copy, Debug)]
pub struct EmergencyStop {
    speed_mm_s: u32,
    duration_ms: u64,
}

impl EmergencyStop {
    #[must_use]
    pub fn new(speed_mm_s: u32, decel_mm_s2: u32) -> Option<Self> {
        if decel_mm_s2 == 0 {
            return None;
        }
        Some(Self {
            speed_mm_s,
            duration_ms: (1000 * u64::from(speed_mm_s)).div_ceil(u64::from(decel_mm_s2)),
        })
    }

    #[must_use]
    pub const fn duration_ms(self) -> u64 {
        self.duration_ms
    }

    #[must_use]
    pub fn sample(self, elapsed_ms: u64) -> (u64, u32) {
        let total = u128::from(self.duration_ms);
        if total == 0 {
            return (0, 0);
        }
        let t = u128::from(elapsed_ms.min(self.duration_ms));
        let v = u128::from(self.speed_mm_s);
        let distance = (v * t * (2 * total - t) + 1000 * total) / (2000 * total);
        let speed = (v * (total - t)).div_ceil(total);
        (u64::try_from(distance).unwrap_or(u64::MAX), saturate(speed))
    }
}
