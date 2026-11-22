use elevator_ctrl::calls::{Press, Rejection};
use elevator_ctrl::dispatcher::{CarInput, Dispatcher, Event};
use elevator_ctrl::eta::{Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams};
use elevator_ctrl::motion::MotionParams;
use elevator_wire::protocol::{CarMode as WireMode, CarSnapshot, DoorState, JSON_SAFE_MAX};

pub const FLOORS: usize = 10;
pub const CARS: usize = 3;
pub const CALLS: usize = 18;

const FLOOR_HEIGHT_MM: u32 = 3000;
const CAR_FLOORS: [u32; CARS] = [1, 5, 10];

type Core = Dispatcher<FLOORS, CARS, CALLS>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressError {
    NotRunning,
    Call(Rejection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarError {
    NoSuchCar,
    WrongState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepEvents {
    pub assigned: usize,
    pub requeued: usize,
}

pub struct Scenario {
    dispatcher: Option<Core>,
    cars: [CarInput; CARS],
    paused: bool,
}

fn eta_params() -> Option<EtaParams> {
    Some(EtaParams {
        building: Building::new(10, FLOOR_HEIGHT_MM)?,
        motion: MotionParams::new(1600, 1000, 1000)?,
        doors: DoorTimes::new(2000, 6000, 3000)?,
        aging_cap_ms: 60_000,
    })
}

fn idle_car(floor: u32) -> CarInput {
    CarInput {
        state: CarState {
            position_mm: (floor - 1) * FLOOR_HEIGHT_MM,
            speed_mm_s: 0,
            accel_mm_s2: 0,
            heading: Direction::Up,
            door_remaining_ms: 0,
            up_calls: 0,
            down_calls: 0,
            car_calls: 0,
            mode: CarMode::Normal,
        },
        stopped_at: None,
        doors_open: false,
        confirmations: [None, None],
    }
}

impl Scenario {
    #[must_use]
    pub fn new() -> Self {
        Self {
            dispatcher: None,
            cars: CAR_FLOORS.map(idle_car),
            paused: false,
        }
    }

    pub fn restart(&mut self, now_ms: u64) -> Option<usize> {
        self.cars = CAR_FLOORS.map(idle_car);
        self.paused = false;
        self.reload(now_ms)
    }

    pub fn fail_car(&mut self, car: usize) -> Result<(), CarError> {
        let input = self.cars.get_mut(car).ok_or(CarError::NoSuchCar)?;
        if input.state.mode == CarMode::Unavailable {
            return Err(CarError::WrongState);
        }
        input.state.mode = CarMode::Unavailable;
        Ok(())
    }

    pub fn recover_car(&mut self, car: usize) -> Result<(), CarError> {
        let input = self.cars.get_mut(car).ok_or(CarError::NoSuchCar)?;
        if input.state.mode != CarMode::Unavailable {
            return Err(CarError::WrongState);
        }
        input.state.mode = CarMode::Normal;
        Ok(())
    }

    pub const fn set_paused(&mut self, paused: bool) -> bool {
        let changed = self.paused != paused;
        self.paused = paused;
        changed
    }

    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn reset(&mut self) -> Option<()> {
        self.cars = CAR_FLOORS.map(idle_car);
        self.paused = false;
        self.start_empty()
    }

    pub fn start_empty(&mut self) -> Option<()> {
        self.dispatcher = Some(Core::new(eta_params()?)?);
        Some(())
    }

    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, PressError> {
        self.dispatcher
            .as_mut()
            .ok_or(PressError::NotRunning)?
            .press(call, now_ms)
            .map_err(PressError::Call)
    }

    pub fn reload(&mut self, now_ms: u64) -> Option<usize> {
        let mut dispatcher = Core::new(eta_params()?)?;
        let mut accepted = 0;
        for floor in 1..=9_u8 {
            let call = Call {
                floor,
                direction: Direction::Up,
            };
            accepted += usize::from(dispatcher.press(call, now_ms).is_ok());
        }
        for floor in 2..=10_u8 {
            let call = Call {
                floor,
                direction: Direction::Down,
            };
            accepted += usize::from(dispatcher.press(call, now_ms).is_ok());
        }
        self.dispatcher = Some(dispatcher);
        Some(accepted)
    }

    pub fn step(&mut self, now_ms: u64) -> StepEvents {
        let mut events = StepEvents::default();
        if self.paused {
            return events;
        }
        if let Some(dispatcher) = self.dispatcher.as_mut() {
            dispatcher.step(now_ms, &self.cars, |event| {
                events.assigned += usize::from(matches!(event, Event::Assigned(_)));
                events.requeued += usize::from(matches!(event, Event::Requeued { .. }));
            });
        }
        events
    }

    pub fn toggle_failure(&mut self, car: usize) -> bool {
        let Some(input) = self.cars.get_mut(car) else {
            return false;
        };
        input.state.mode = if input.state.mode == CarMode::Unavailable {
            CarMode::Normal
        } else {
            CarMode::Unavailable
        };
        input.state.mode == CarMode::Unavailable
    }

    #[must_use]
    pub fn car_snapshot(&self, car: usize, now_ms: u64) -> Option<CarSnapshot> {
        let state = self.cars.get(car)?.state;
        let on_floor = state.position_mm % FLOOR_HEIGHT_MM == 0;
        let floor = on_floor
            .then(|| u8::try_from(state.position_mm / FLOOR_HEIGHT_MM + 1).ok())
            .flatten();
        let speed = i32::try_from(state.speed_mm_s).unwrap_or(i32::MAX);
        Some(CarSnapshot {
            car: u8::try_from(car).ok()?,
            mode: match state.mode {
                CarMode::Normal => WireMode::Normal,
                CarMode::FireRecall => WireMode::FireRecall,
                CarMode::Firefighters => WireMode::Firefighters,
                CarMode::Unavailable => WireMode::Unavailable,
            },
            doors: if state.door_remaining_ms == 0 {
                DoorState::Closed
            } else {
                DoorState::Open
            },
            floor,
            position_mm: state.position_mm,
            speed_mm_per_sec: match state.heading {
                Direction::Up => speed,
                Direction::Down => -speed,
            },
            eta_ms: None,
            up_calls: state.up_calls,
            down_calls: state.down_calls,
            model_time_ms: now_ms.min(JSON_SAFE_MAX),
        })
    }

    #[must_use]
    pub fn is_failed(&self, car: usize) -> bool {
        self.cars
            .get(car)
            .is_some_and(|input| input.state.mode == CarMode::Unavailable)
    }
}

impl Default for Scenario {
    fn default() -> Self {
        Self::new()
    }
}
