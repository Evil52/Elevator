use crate::Cell;
use crate::motion::MotionParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarMode {
    Normal,
    FireRecall,
    Firefighters,
    Unavailable,
}

impl CarMode {
    #[must_use]
    pub const fn accepts_assignments(self) -> bool {
        matches!(self, Self::Normal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub floor: u8,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedCall {
    pub call: Call,
    pub waited_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    floors: u8,
    floor_height_mm: u32,
}

impl Building {
    #[must_use]
    pub const fn new(floors: u8, floor_height_mm: u32) -> Option<Self> {
        if floors == 0 || floors > 32 || floor_height_mm == 0 {
            return None;
        }
        if (floors as u32 - 1).checked_mul(floor_height_mm).is_none() {
            return None;
        }
        Some(Self {
            floors,
            floor_height_mm,
        })
    }

    #[must_use]
    pub const fn floors(self) -> u8 {
        self.floors
    }

    const fn position_mm(self, floor: u32) -> u32 {
        (floor - 1) * self.floor_height_mm
    }

    const fn top_position_mm(self) -> u32 {
        self.position_mm(self.floors as u32)
    }

    const fn all_floors(self) -> u32 {
        if self.floors == 32 {
            u32::MAX
        } else {
            (1 << self.floors) - 1
        }
    }

    const fn mirror_mask(self, mask: u32) -> u32 {
        mask.reverse_bits() >> (32 - self.floors as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorTimes {
    open: u32,
    dwell: u32,
    close: u32,
}

impl DoorTimes {
    pub const DWELL_RANGE_MS: core::ops::RangeInclusive<u32> = 2000..=20_000;

    #[must_use]
    pub fn new(open_ms: u32, dwell_ms: u32, close_ms: u32) -> Option<Self> {
        if open_ms == 0 || close_ms == 0 || !Self::DWELL_RANGE_MS.contains(&dwell_ms) {
            return None;
        }
        Some(Self {
            open: open_ms,
            dwell: dwell_ms,
            close: close_ms,
        })
    }

    #[must_use]
    pub const fn cycle_ms(self) -> u64 {
        self.open as u64 + self.dwell as u64 + self.close as u64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EtaParams {
    pub building: Building,
    pub motion: MotionParams,
    pub doors: DoorTimes,
    pub aging_cap_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarState {
    pub position_mm: u32,
    pub speed_mm_s: u32,
    pub accel_mm_s2: u32,
    pub heading: Direction,
    pub door_remaining_ms: u32,
    pub up_calls: u32,
    pub down_calls: u32,
    pub car_calls: u32,
    pub mode: CarMode,
}

#[must_use]
pub fn eta_ms(params: &EtaParams, car: &CarState, call: Call) -> Option<u32> {
    let building = params.building;
    let floor = u32::from(call.floor);
    let valid = car.mode.accepts_assignments()
        && (1..=u32::from(building.floors)).contains(&floor)
        && car.position_mm <= building.top_position_mm()
        && params
            .motion
            .stopping_distance_mm(car.speed_mm_s, car.accel_mm_s2)
            .is_some()
        && (car.up_calls | car.down_calls | car.car_calls) & !building.all_floors() == 0;
    if !valid {
        return None;
    }
    match car.heading {
        Direction::Up => Route::new(params, car, call).run(),
        Direction::Down => {
            let mirrored = CarState {
                position_mm: building.top_position_mm() - car.position_mm,
                heading: Direction::Up,
                up_calls: building.mirror_mask(car.down_calls),
                down_calls: building.mirror_mask(car.up_calls),
                car_calls: building.mirror_mask(car.car_calls),
                ..*car
            };
            let mirrored_call = Call {
                floor: building.floors + 1 - call.floor,
                direction: call.direction.opposite(),
            };
            Route::new(params, &mirrored, mirrored_call).run()
        }
    }
}

fn saturate(millis: u64) -> u32 {
    u32::try_from(millis).unwrap_or(u32::MAX)
}

struct Clock<'a> {
    params: &'a EtaParams,
    millis: u64,
    position_mm: u32,
    speed_mm_s: u32,
    accel_mm_s2: u32,
}

impl Clock<'_> {
    const fn moving(&self) -> bool {
        self.speed_mm_s > 0 || self.accel_mm_s2 > 0
    }

    fn drive_to(&mut self, floor: u32) -> Option<u64> {
        let target = self.params.building.position_mm(floor);
        let travel = self.params.motion.time_to_stop_from_ms(
            target.abs_diff(self.position_mm),
            self.speed_mm_s,
            self.accel_mm_s2,
        )?;
        self.millis += u64::from(travel);
        self.position_mm = target;
        self.speed_mm_s = 0;
        self.accel_mm_s2 = 0;
        Some(self.millis)
    }

    fn cycle_doors(&mut self) {
        self.millis += self.params.doors.cycle_ms();
    }
}

const fn floor_bit(floor: u32) -> u32 {
    1 << (floor - 1)
}

struct Route<'a> {
    clock: Clock<'a>,
    call: Call,
    floors: u32,
    first_ahead: u32,
    start_position_mm: u32,
    up: u32,
    down: u32,
    car: u32,
    turn_floor: Option<u32>,
    lowest: u32,
    found: Option<u32>,
}

impl<'a> Route<'a> {
    fn new(params: &'a EtaParams, car: &CarState, call: Call) -> Self {
        let building = params.building;
        let target = floor_bit(u32::from(call.floor));
        let (mut up, mut down) = (car.up_calls, car.down_calls);
        match call.direction {
            Direction::Up => up |= target,
            Direction::Down => down |= target,
        }
        let moving = car.speed_mm_s > 0 || car.accel_mm_s2 > 0;
        let stopping = params
            .motion
            .stopping_distance_mm(car.speed_mm_s, car.accel_mm_s2)
            .unwrap_or(u32::MAX);
        let reach = u64::from(car.position_mm) + u64::from(stopping);
        let first_ahead = reach.div_ceil(u64::from(building.floor_height_mm)) + 1;
        Self {
            clock: Clock {
                params,
                millis: if moving {
                    0
                } else {
                    u64::from(car.door_remaining_ms)
                },
                position_mm: car.position_mm,
                speed_mm_s: car.speed_mm_s,
                accel_mm_s2: car.accel_mm_s2,
            },
            call,
            floors: u32::from(building.floors),
            first_ahead: saturate(first_ahead),
            start_position_mm: car.position_mm,
            up,
            down,
            car: car.car_calls,
            turn_floor: None,
            lowest: 0,
            found: None,
        }
    }

    fn run(mut self) -> Option<u32> {
        self.sweep_up()?;
        if self.found.is_none() {
            self.sweep_down()?;
        }
        if self.found.is_none() {
            self.sweep_up_again()?;
        }
        self.found
    }

    fn visit(&mut self, floor: u32, served_up: bool, served_down: bool) -> Option<()> {
        let arrival = self.clock.drive_to(floor)?;
        let served = match self.call.direction {
            Direction::Up => served_up,
            Direction::Down => served_down,
        };
        if floor == u32::from(self.call.floor) && served {
            self.found = Some(saturate(arrival));
        } else {
            self.clock.cycle_doors();
        }
        Some(())
    }

    fn sweep_up(&mut self) -> Option<()> {
        let ahead = self.up_ahead_mask();
        let pending = (self.up | self.down | self.car) & ahead;
        if pending == 0 {
            if self.clock.moving() {
                if self.first_ahead > self.floors {
                    return None;
                }
                self.clock.drive_to(self.first_ahead)?;
                self.turn_floor = Some(self.first_ahead);
            }
            return Some(());
        }
        let top = 32 - pending.leading_zeros();
        self.turn_floor = Some(top);
        for floor in self.first_ahead..=top {
            let bit = floor_bit(floor);
            let at_top = floor == top;
            if (self.up | self.car) & bit != 0 || at_top {
                self.visit(floor, self.up & bit != 0, at_top && self.down & bit != 0)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.up &= !bit;
                self.car &= !bit;
                if at_top {
                    self.down &= !bit;
                }
            }
        }
        Some(())
    }

    fn up_ahead_mask(&self) -> u32 {
        let all = self.clock.params.building.all_floors();
        all & u32::MAX.checked_shl(self.first_ahead - 1).unwrap_or(0)
    }

    fn sweep_down(&mut self) -> Option<()> {
        let building = self.clock.params.building;
        let turn_position = self
            .turn_floor
            .map_or(self.start_position_mm, |floor| building.position_mm(floor));
        let below = turn_position.div_ceil(building.floor_height_mm);
        let below_mask = u32::MAX.checked_shr(32 - below.min(32)).unwrap_or(0);
        let rest_down = self.down & below_mask;
        let rest = rest_down | self.up | self.car;
        if rest == 0 {
            return None;
        }
        self.lowest = rest.trailing_zeros() + 1;
        for floor in (self.lowest..=below).rev() {
            let bit = floor_bit(floor);
            let at_lowest = floor == self.lowest;
            if (rest_down | self.car) & bit != 0 || at_lowest {
                self.visit(floor, at_lowest && self.up & bit != 0, rest_down & bit != 0)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.down &= !bit;
                self.car &= !bit;
                if at_lowest {
                    self.up &= !bit;
                }
            }
        }
        Some(())
    }

    fn sweep_up_again(&mut self) -> Option<()> {
        for floor in (self.lowest + 1)..=self.floors {
            let bit = floor_bit(floor);
            if self.up & bit != 0 {
                self.visit(floor, true, false)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.up &= !bit;
            }
        }
        None
    }
}

#[must_use]
pub fn build_matrix<const CARS: usize, const CALLS: usize>(
    params: &EtaParams,
    cars: &[CarState; CARS],
    calls: &[QueuedCall; CALLS],
) -> [[Cell; CALLS]; CARS] {
    core::array::from_fn(|car| {
        core::array::from_fn(|index| {
            let queued = calls[index];
            match eta_ms(params, &cars[car], queued.call) {
                Some(eta) => {
                    let cap = params.aging_cap_ms;
                    let aging = cap - queued.waited_ms.min(cap);
                    Cell::allowed(eta.saturating_add(aging))
                }
                None => Cell::forbidden(0),
            }
        })
    })
}
