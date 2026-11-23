use elevator_ctrl::eta::{CarMode, Direction};
use elevator_ctrl::motion::{EmergencyStop, MotionParams, MovingTrip};
use elevator_wire::protocol::DoorState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlantParams {
    pub floors: u8,
    pub floor_height_mm: u32,
    pub motion: MotionParams,
    pub open_ms: u32,
    pub dwell_ms: u32,
    pub close_ms: u32,
    pub brake_mm_s2: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Targets {
    pub up: u32,
    pub down: u32,
}

#[derive(Clone, Copy, Debug)]
enum Drive {
    Rest,
    Trip {
        from_mm: u32,
        to_mm: u32,
        trip: MovingTrip,
        elapsed_ms: u32,
    },
    Braking {
        from_mm: u32,
        brake: EmergencyStop,
        elapsed_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cycle {
    Closed,
    Opening(u32),
    Open(u32),
    Closing(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Car {
    params: PlantParams,
    drive: Drive,
    cycle: Cycle,
    position_mm: u32,
    speed_mm_s: u32,
    accel_mm_s2: i64,
    heading: Direction,
}

impl Car {
    #[must_use]
    pub const fn new(params: PlantParams, floor: u8) -> Self {
        Self {
            params,
            drive: Drive::Rest,
            cycle: Cycle::Closed,
            position_mm: (floor as u32).saturating_sub(1) * params.floor_height_mm,
            speed_mm_s: 0,
            accel_mm_s2: 0,
            heading: Direction::Up,
        }
    }

    pub fn step(&mut self, dt_ms: u32, mode: CarMode, targets: Targets) {
        match self.drive {
            Drive::Trip { .. } => self.drive_trip(dt_ms, mode, targets),
            Drive::Braking { .. } => self.drive_braking(dt_ms),
            Drive::Rest => {
                self.advance_doors(dt_ms);
                if mode != CarMode::Unavailable && self.cycle == Cycle::Closed {
                    self.depart(dt_ms, mode, targets);
                }
            }
        }
    }

    #[must_use]
    pub const fn position_mm(&self) -> u32 {
        self.position_mm
    }

    #[must_use]
    pub const fn speed_mm_s(&self) -> u32 {
        self.speed_mm_s
    }

    #[must_use]
    pub const fn accel_mm_s2(&self) -> i64 {
        self.accel_mm_s2
    }

    #[must_use]
    pub const fn heading(&self) -> Direction {
        self.heading
    }

    #[must_use]
    pub const fn doors(&self) -> DoorState {
        match self.cycle {
            Cycle::Closed => DoorState::Closed,
            Cycle::Opening(_) => DoorState::Opening,
            Cycle::Open(_) => DoorState::Open,
            Cycle::Closing(_) => DoorState::Closing,
        }
    }

    #[must_use]
    pub const fn door_remaining_ms(&self) -> u32 {
        let p = &self.params;
        match self.cycle {
            Cycle::Closed => 0,
            Cycle::Opening(e) => (p.open_ms - e)
                .saturating_add(p.dwell_ms)
                .saturating_add(p.close_ms),
            Cycle::Open(e) => (p.dwell_ms - e).saturating_add(p.close_ms),
            Cycle::Closing(e) => p.close_ms - e,
        }
    }

    #[must_use]
    pub fn stopped_at(&self) -> Option<u8> {
        let height = self.params.floor_height_mm;
        if !matches!(self.drive, Drive::Rest)
            || height == 0
            || !self.position_mm.is_multiple_of(height)
        {
            return None;
        }
        u8::try_from(self.position_mm / height + 1).ok()
    }

    fn floor_position(&self, floor: u32) -> u32 {
        (floor - 1) * self.params.floor_height_mm
    }

    fn top_position(&self) -> u32 {
        self.floor_position(u32::from(self.params.floors))
    }

    fn advance_doors(&mut self, dt_ms: u32) {
        let p = self.params;
        let mut left = u64::from(dt_ms);
        loop {
            let (length, next) = match self.cycle {
                Cycle::Closed => return,
                Cycle::Opening(e) => (p.open_ms - e, Some(Cycle::Open(0))),
                Cycle::Open(e) => (p.dwell_ms - e, Some(Cycle::Closing(0))),
                Cycle::Closing(e) => (p.close_ms - e, None),
            };
            if left < u64::from(length) {
                let moved = u32::try_from(left).unwrap_or(length);
                self.cycle = match self.cycle {
                    Cycle::Opening(e) => Cycle::Opening(e + moved),
                    Cycle::Open(e) => Cycle::Open(e + moved),
                    Cycle::Closing(e) => Cycle::Closing(e + moved),
                    Cycle::Closed => Cycle::Closed,
                };
                return;
            }
            left -= u64::from(length);
            self.cycle = next.unwrap_or(Cycle::Closed);
        }
    }

    fn depart(&mut self, dt_ms: u32, mode: CarMode, targets: Targets) {
        let all = targets.up | targets.down;
        if all == 0 {
            return;
        }
        if let Some(floor) = self.stopped_at()
            && all & (1 << (floor - 1)) != 0
        {
            self.cycle = Cycle::Opening(0);
            return;
        }
        let Some((floor, heading)) = self.pick(targets) else {
            return;
        };
        let to_mm = self.floor_position(floor);
        let Some(trip) = self
            .params
            .motion
            .moving_trip(self.position_mm.abs_diff(to_mm), 0, 0)
        else {
            return;
        };
        self.heading = heading;
        self.drive = Drive::Trip {
            from_mm: self.position_mm,
            to_mm,
            trip,
            elapsed_ms: 0,
        };
        self.drive_trip(dt_ms, mode, targets);
    }

    fn retarget(&mut self, targets: Targets) {
        let Drive::Trip { to_mm, .. } = self.drive else {
            return;
        };
        let motion = self.params.motion;
        let Ok(accel) = u32::try_from(self.accel_mm_s2) else {
            return;
        };
        let speed = self.speed_mm_s;
        let Some(stop) = motion.stopping_distance_mm(speed, accel) else {
            return;
        };
        let own = match self.heading {
            Direction::Up => targets.up,
            Direction::Down => targets.down,
        };
        let position = self.position_mm;
        let reachable = |floor: &u32| {
            let at = self.floor_position(*floor);
            own & (1 << (*floor - 1)) != 0
                && match self.heading {
                    Direction::Up => at < to_mm && at >= position.saturating_add(stop),
                    Direction::Down => at > to_mm && at.saturating_add(stop) <= position,
                }
        };
        let floors = 1..=u32::from(self.params.floors);
        let next = match self.heading {
            Direction::Up => floors.clone().find(reachable),
            Direction::Down => floors.rev().find(reachable),
        };
        let Some(floor) = next else {
            return;
        };
        let to_mm = self.floor_position(floor);
        if let Some(trip) = motion.moving_trip(position.abs_diff(to_mm), speed, accel) {
            self.drive = Drive::Trip {
                from_mm: position,
                to_mm,
                trip,
                elapsed_ms: 0,
            };
        }
    }

    fn pick(&self, targets: Targets) -> Option<(u32, Direction)> {
        let opposite = match self.heading {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        [self.heading, opposite]
            .into_iter()
            .find_map(|direction| Some((self.pick_towards(direction, targets)?, direction)))
    }

    fn pick_towards(&self, direction: Direction, targets: Targets) -> Option<u32> {
        let own = match direction {
            Direction::Up => targets.up,
            Direction::Down => targets.down,
        };
        let any = targets.up | targets.down;
        let ahead = |floor: &u32| match direction {
            Direction::Up => self.floor_position(*floor) > self.position_mm,
            Direction::Down => self.floor_position(*floor) < self.position_mm,
        };
        let marked = |mask: u32| move |floor: &u32| mask & (1 << (*floor - 1)) != 0;
        let floors = || (1..=u32::from(self.params.floors)).filter(ahead);
        match direction {
            Direction::Up => floors()
                .find(marked(own))
                .or_else(|| floors().rev().find(marked(any))),
            Direction::Down => floors()
                .rev()
                .find(marked(own))
                .or_else(|| floors().find(marked(any))),
        }
    }

    fn drive_trip(&mut self, dt_ms: u32, mode: CarMode, targets: Targets) {
        if mode == CarMode::Unavailable {
            self.start_braking(dt_ms);
            return;
        }
        self.retarget(targets);
        let Drive::Trip {
            from_mm,
            to_mm,
            trip,
            elapsed_ms,
        } = self.drive
        else {
            return;
        };
        let elapsed_ms = elapsed_ms.saturating_add(dt_ms);
        let (travelled, speed, accel) = trip.sample(elapsed_ms);
        let speed = speed.min(self.params.motion.max_speed_mm_s());
        let accel = accel.min(i64::from(self.accel_limit(speed)));
        self.position_mm = if to_mm >= from_mm {
            from_mm + travelled
        } else {
            from_mm - travelled
        };
        self.speed_mm_s = speed;
        self.accel_mm_s2 = accel;
        if elapsed_ms >= trip.duration_ms() {
            self.position_mm = to_mm;
            self.speed_mm_s = 0;
            self.drive = Drive::Rest;
            self.cycle = Cycle::Opening(0);
        } else {
            self.drive = Drive::Trip {
                from_mm,
                to_mm,
                trip,
                elapsed_ms,
            };
        }
    }

    fn accel_limit(&self, speed_mm_s: u32) -> u32 {
        let motion = self.params.motion;
        let headroom = u64::from(motion.max_speed_mm_s().saturating_sub(speed_mm_s));
        let reachable = (2 * u64::from(motion.jerk_mm_s3()) * headroom).isqrt();
        u32::try_from(reachable)
            .unwrap_or(u32::MAX)
            .min(motion.accel_mm_s2())
    }

    fn start_braking(&mut self, dt_ms: u32) {
        let Some(brake) = EmergencyStop::new(self.speed_mm_s, self.params.brake_mm_s2) else {
            self.speed_mm_s = 0;
            self.accel_mm_s2 = 0;
            self.drive = Drive::Rest;
            return;
        };
        self.drive = Drive::Braking {
            from_mm: self.position_mm,
            brake,
            elapsed_ms: 0,
        };
        self.drive_braking(dt_ms);
    }

    fn drive_braking(&mut self, dt_ms: u32) {
        let Drive::Braking {
            from_mm,
            brake,
            elapsed_ms,
        } = self.drive
        else {
            return;
        };
        let elapsed_ms = elapsed_ms.saturating_add(u64::from(dt_ms));
        let (distance, speed) = brake.sample(elapsed_ms);
        let distance = u32::try_from(distance).unwrap_or(u32::MAX);
        self.position_mm = match self.heading {
            Direction::Up => from_mm.saturating_add(distance).min(self.top_position()),
            Direction::Down => from_mm.saturating_sub(distance),
        };
        self.speed_mm_s = speed;
        self.accel_mm_s2 = -i64::from(self.params.brake_mm_s2);
        if elapsed_ms >= brake.duration_ms() {
            self.speed_mm_s = 0;
            self.accel_mm_s2 = 0;
            self.drive = Drive::Rest;
        } else {
            self.drive = Drive::Braking {
                from_mm,
                brake,
                elapsed_ms,
            };
        }
    }
}
