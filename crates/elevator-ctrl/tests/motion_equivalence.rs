#![allow(dead_code)]

mod reference {
    const UM: u128 = 1000;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Reference {
        max_speed_mm_s: u32,
        accel_mm_s2: u32,
        jerk_mm_s3: u32,
    }

    impl Reference {
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
                settle.millis + self.stop_plan_um(rest, settle.speed)?.duration_ms(),
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

        fn stop_plan_um(&self, distance: u128, start: u128) -> Option<StopPlan> {
            if self.ramp(0, start).1 > distance {
                return None;
            }
            if distance == 0 {
                return Some(StopPlan {
                    ramps_ms: 0,
                    cruise_ms: 0,
                });
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
            Some(StopPlan {
                ramps_ms,
                cruise_ms,
            })
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

    struct StopPlan {
        ramps_ms: u128,
        cruise_ms: u128,
    }

    impl StopPlan {
        const fn duration_ms(&self) -> u128 {
            self.ramps_ms + self.cruise_ms
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
}

use elevator_ctrl::motion::MotionParams;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, bound: u32) -> u32 {
        u32::try_from(self.next() % u64::from(bound.max(1))).unwrap_or(0)
    }

    fn pick(&mut self, small: u32, any: u32) -> u32 {
        match self.next() % 4 {
            0 => self.below(small) + 1,
            1 => self.below(any) + 1,
            2 => any - self.below(3),
            _ => self.below(4000) + 1,
        }
    }
}

#[test]
fn optimised_motion_equals_the_reference_bit_for_bit() {
    let mut rng = Lcg(0xE1E7);
    let mut compared = 0_u32;
    for _ in 0..60_000 {
        let vmax = rng.pick(3000, u32::MAX);
        let accel = rng.pick(2000, u32::MAX);
        let jerk = rng.pick(2000, u32::MAX);
        let (Some(fast), Some(slow)) = (
            MotionParams::new(vmax, accel, jerk),
            reference::Reference::new(vmax, accel, jerk),
        ) else {
            continue;
        };
        for _ in 0..8 {
            let speed = match rng.next() % 3 {
                0 => 0,
                1 => rng.below(vmax.saturating_add(1)),
                _ => vmax.saturating_sub(rng.below(3)),
            };
            let accel0 = match rng.next() % 3 {
                0 => 0,
                1 => rng.below(accel.saturating_add(1)),
                _ => accel.saturating_sub(rng.below(3)),
            };
            let distance = match rng.next() % 4 {
                0 => rng.below(40_000),
                1 => rng.below(500_000),
                2 => rng.below(u32::MAX),
                _ => u32::MAX - rng.below(3),
            };
            if accel0 == 0 {
                assert_eq!(
                    fast.time_to_stop_from_ms(distance, speed, accel0),
                    slow.time_to_stop_from_ms(distance, speed, accel0),
                    "time vmax={vmax} accel={accel} jerk={jerk} d={distance} v={speed} a={accel0}"
                );
            } else {
                assert_eq!(
                    fast.time_to_stop_from_ms(distance, speed, accel0).is_some(),
                    slow.time_to_stop_from_ms(distance, speed, accel0).is_some(),
                    "feasible vmax={vmax} accel={accel} jerk={jerk} d={distance} v={speed} a={accel0}"
                );
            }
            assert_eq!(
                fast.stopping_distance_mm(speed, accel0),
                slow.stopping_distance_mm(speed, accel0),
                "stopping vmax={vmax} accel={accel} jerk={jerk} v={speed} a={accel0}"
            );
            assert_eq!(
                fast.braking_distance_mm(speed),
                slow.braking_distance_mm(speed),
                "braking vmax={vmax} accel={accel} jerk={jerk} v={speed}"
            );
            compared += 1;
        }
    }
    assert!(compared > 100_000, "compared only {compared}");
}
