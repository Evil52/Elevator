use super::{MotionParams, div_ceil};

#[derive(Clone, Copy, Debug)]
enum Shape {
    Ramp {
        from: u64,
        delta: u64,
        rise: u32,
        down: bool,
    },
    Cruise {
        speed: u64,
    },
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    shape: Shape,
    length_ms: u32,
    offset_ms: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct MovingTrip {
    distance_mm: u32,
    duration_ms: u32,
    total_nm: u64,
    pieces: [Piece; 3],
}

struct Point {
    path_nm: u128,
    speed_num: u128,
    speed_den: u128,
    accel_num: u128,
    accel_den: u128,
    braking: bool,
}

impl MotionParams {
    #[must_use]
    pub const fn accel_mm_s2(&self) -> u32 {
        self.accel_mm_s2
    }

    #[must_use]
    pub const fn jerk_mm_s3(&self) -> u32 {
        self.jerk_mm_s3
    }

    #[must_use]
    pub fn moving_trip(
        &self,
        distance_mm: u32,
        speed_mm_s: u32,
        accel_mm_s2: u32,
    ) -> Option<MovingTrip> {
        let moving = self.moving_plan(distance_mm, speed_mm_s, accel_mm_s2)?;
        let duration_ms = u32::try_from(moving.duration_ms()).ok()?;
        let start = u64::try_from(moving.start).ok()?;
        let cruise = u64::try_from(moving.plan.cruise).ok()?;
        let (accel_ms, _) = self.ramp(moving.start, moving.plan.cruise);
        let (brake_ms, _) = self.ramp(0, moving.plan.cruise);
        let pieces = [
            Piece {
                shape: Shape::Ramp {
                    from: start,
                    delta: cruise - start,
                    rise: self.rise_ms(accel_ms)?,
                    down: false,
                },
                length_ms: u32::try_from(accel_ms.saturating_sub(moving.launch_ms)).ok()?,
                offset_ms: u32::try_from(moving.launch_ms.min(accel_ms)).ok()?,
            },
            Piece {
                shape: Shape::Cruise { speed: cruise },
                length_ms: u32::try_from(moving.plan.cruise_ms).ok()?,
                offset_ms: 0,
            },
            Piece {
                shape: Shape::Ramp {
                    from: cruise,
                    delta: cruise,
                    rise: self.rise_ms(brake_ms)?,
                    down: true,
                },
                length_ms: u32::try_from(brake_ms).ok()?,
                offset_ms: 0,
            },
        ];
        let mut total_nm = 0u64;
        for piece in &pieces {
            total_nm = total_nm.checked_add(piece.full_nm()?)?;
        }
        let trip = MovingTrip {
            distance_mm,
            duration_ms,
            total_nm: total_nm.max(1),
            pieces,
        };
        trip.representable().then_some(trip)
    }

    fn rise_ms(&self, ramp_ms: u128) -> Option<u32> {
        let rise = div_ceil(
            1000 * u128::from(self.accel_mm_s2),
            u128::from(self.jerk_mm_s3),
        )
        .min(ramp_ms / 2);
        u32::try_from(rise).ok()
    }
}

impl MovingTrip {
    #[must_use]
    pub const fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    #[must_use]
    pub fn sample(&self, elapsed_ms: u32) -> (u32, u32, i64) {
        let done = (self.distance_mm, 0, 0);
        if elapsed_ms >= self.duration_ms {
            return done;
        }
        let mut local = elapsed_ms;
        let mut before_nm = 0u64;
        for piece in &self.pieces {
            if local < piece.length_ms {
                return self.scaled(piece, before_nm, local).unwrap_or(done);
            }
            local -= piece.length_ms;
            let Some(after) = piece.full_nm().and_then(|nm| before_nm.checked_add(nm)) else {
                return done;
            };
            before_nm = after;
        }
        done
    }

    fn representable(&self) -> bool {
        let mut before_nm = 0u64;
        for piece in &self.pieces {
            if piece.length_ms > 0
                && piece
                    .checkpoints()
                    .iter()
                    .any(|&local| self.scaled(piece, before_nm, local).is_none())
            {
                return false;
            }
            let Some(after) = piece.full_nm().and_then(|nm| before_nm.checked_add(nm)) else {
                return false;
            };
            before_nm = after;
        }
        true
    }

    fn scaled(&self, piece: &Piece, before_nm: u64, local: u32) -> Option<(u32, u32, i64)> {
        let point = piece.point(u128::from(local))?;
        let distance = u128::from(self.distance_mm);
        let total = u128::from(self.total_nm);
        let path = u128::from(before_nm).checked_add(point.path_nm)?;
        let travelled = nearest(distance.checked_mul(path)?, total)?;
        let speed =
            (distance * 1000).checked_mul(point.speed_num)? / point.speed_den.checked_mul(total)?;
        let accel = (distance * 1_000_000).checked_mul(point.accel_num)?
            / point.accel_den.checked_mul(total)?;
        let accel = i64::try_from(accel).ok()?;
        Some((
            u32::try_from(travelled).ok()?,
            u32::try_from(speed).ok()?,
            if point.braking { -accel } else { accel },
        ))
    }
}

impl Piece {
    fn checkpoints(&self) -> [u32; 4] {
        let length = self.length_ms;
        match self.shape {
            Shape::Ramp { rise, .. } => {
                let whole = length + self.offset_ms;
                let local = |at: u32| at.saturating_sub(self.offset_ms).min(length);
                [0, local(rise), local(whole - rise), length]
            }
            Shape::Cruise { .. } => [0, length, length, length],
        }
    }

    fn full_nm(&self) -> Option<u64> {
        if self.length_ms == 0 {
            return Some(0);
        }
        u64::try_from(self.point(u128::from(self.length_ms))?.path_nm).ok()
    }

    fn point(&self, t: u128) -> Option<Point> {
        match self.shape {
            Shape::Cruise { speed } => {
                let speed = u128::from(speed);
                Some(Point {
                    path_nm: speed.checked_mul(t)?,
                    speed_num: speed,
                    speed_den: 1,
                    accel_num: 0,
                    accel_den: 1,
                    braking: false,
                })
            }
            Shape::Ramp {
                from,
                delta,
                rise,
                down,
            } => {
                let whole = u128::from(self.length_ms) + u128::from(self.offset_ms);
                let at = |t: u128| {
                    ramp_point(
                        u128::from(from),
                        u128::from(delta),
                        whole,
                        u128::from(rise),
                        t,
                        down,
                    )
                };
                let mut point = at(t + u128::from(self.offset_ms))?;
                if self.offset_ms > 0 {
                    let base = at(u128::from(self.offset_ms))?;
                    point.path_nm = point.path_nm.saturating_sub(base.path_nm);
                }
                Some(point)
            }
        }
    }
}

fn ramp_point(
    from: u128,
    delta: u128,
    length: u128,
    rise: u128,
    t: u128,
    down: bool,
) -> Option<Point> {
    let (path_den, path_shape, speed_den, speed_shape, accel_num, accel_den) = if rise == 0 {
        (2 * length, t * t, length, t, 1, length)
    } else {
        let r = rise;
        let k = length - r;
        let rk = r.checked_mul(k)?;
        let back = length - t;
        let (path_shape, speed_shape, accel_num) = if t <= r {
            (t.checked_mul(t)?.checked_mul(t)?, t * t, t)
        } else if t >= length - r {
            let grown = (6 * rk)
                .checked_mul(t)?
                .checked_add(back.checked_mul(back)?.checked_mul(back)?)?;
            (
                grown - (3 * rk).checked_mul(length)?,
                2 * rk - back * back,
                back,
            )
        } else {
            let grown = (3 * r)
                .checked_mul(t)?
                .checked_mul(t)?
                .checked_add(r.checked_mul(r)?.checked_mul(r)?)?;
            (grown - 3 * r * r * t, 2 * r * t - r * r, r)
        };
        (6 * rk, path_shape, 2 * rk, speed_shape, accel_num, rk)
    };
    let base_path = from.checked_mul(t)?.checked_mul(path_den)?;
    let change_path = delta.checked_mul(path_shape)?;
    let base_speed = from.checked_mul(speed_den)?;
    let change_speed = delta.checked_mul(speed_shape)?;
    let (path, speed) = if down {
        (
            base_path.checked_sub(change_path)?,
            base_speed.checked_sub(change_speed)?,
        )
    } else {
        (
            base_path.checked_add(change_path)?,
            base_speed.checked_add(change_speed)?,
        )
    };
    Some(Point {
        path_nm: path / path_den,
        speed_num: speed,
        speed_den,
        accel_num: delta.checked_mul(accel_num)?,
        accel_den,
        braking: down,
    })
}

fn nearest(n: u128, d: u128) -> Option<u128> {
    Some(n.checked_add(d / 2)? / d)
}
