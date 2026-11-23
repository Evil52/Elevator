use elevator_ctrl::calls::{Assignment, CallState, CallTable, CarId, Press, Rejection};
use elevator_ctrl::dispatcher::{CarInput, Dispatcher, Event};
use elevator_ctrl::eta::{Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams};
use elevator_ctrl::motion::MotionParams;
use elevator_wire::protocol::{
    AlarmKind, AlarmPage, CarMode as WireMode, CarSnapshot, DoorState, EventKind, EventLogChunk,
    JSON_SAFE_MAX,
};

use crate::alarms::{AlarmError, AlarmTable, Raised};
use crate::car::{Car, PlantParams, Targets};
use crate::events::EventLog;

const FLOOR_COUNT: u8 = 10;
pub const FLOORS: usize = FLOOR_COUNT as usize;
pub const CARS: usize = 3;
pub const CALLS: usize = 18;
pub const LOG_CAPACITY: usize = 64;
pub const ALARM_CAPACITY: usize = 16;

const FLOOR_HEIGHT_MM: u32 = 3000;
const DOOR_OPEN_MS: u32 = 2000;
const DOOR_DWELL_MS: u32 = 6000;
const DOOR_CLOSE_MS: u32 = 3000;
const BRAKE_MM_S2: u32 = 3000;
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
    plants: Option<[Car; CARS]>,
    doors: [DoorState; CARS],
    assigned: [Targets; CARS],
    last_step_ms: u64,
    paused: bool,
    log: EventLog<LOG_CAPACITY>,
    alarms: AlarmTable<ALARM_CAPACITY>,
    alarm_version: u32,
    clock_ms: u64,
}

fn motion_params() -> Option<MotionParams> {
    MotionParams::new(1600, 1000, 1000)
}

fn eta_params() -> Option<EtaParams> {
    Some(EtaParams {
        building: Building::new(10, FLOOR_HEIGHT_MM)?,
        motion: motion_params()?,
        doors: DoorTimes::new(DOOR_OPEN_MS, DOOR_DWELL_MS, DOOR_CLOSE_MS)?,
        aging_cap_ms: 60_000,
    })
}

fn plant_params() -> Option<PlantParams> {
    Some(PlantParams {
        floors: FLOOR_COUNT,
        floor_height_mm: FLOOR_HEIGHT_MM,
        motion: motion_params()?,
        open_ms: DOOR_OPEN_MS,
        dwell_ms: DOOR_DWELL_MS,
        close_ms: DOOR_CLOSE_MS,
        brake_mm_s2: BRAKE_MM_S2,
    })
}

fn new_plants() -> Option<[Car; CARS]> {
    let params = plant_params()?;
    Some(CAR_FLOORS.map(|floor| Car::new(params, u8::try_from(floor).unwrap_or(1))))
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
            plants: None,
            doors: [DoorState::Closed; CARS],
            assigned: [Targets::default(); CARS],
            last_step_ms: 0,
            paused: false,
            log: EventLog::new(),
            alarms: AlarmTable::new(),
            alarm_version: 0,
            clock_ms: 0,
        }
    }

    pub fn restart(&mut self, now_ms: u64) -> Option<usize> {
        self.reset_cars();
        self.last_step_ms = now_ms;
        self.paused = false;
        self.forget_history();
        self.reload(now_ms)
    }

    fn reset_cars(&mut self) {
        self.cars = CAR_FLOORS.map(idle_car);
        self.plants = new_plants();
        self.doors = [DoorState::Closed; CARS];
        self.assigned = [Targets::default(); CARS];
    }

    fn forget_history(&mut self) {
        self.log = EventLog::new();
        self.alarms = AlarmTable::new();
        self.alarm_version = self.alarm_version.wrapping_add(1);
    }

    fn record(&mut self, kind: EventKind, car: Option<u8>, id: u32) {
        if self.log.push(self.clock_ms, kind, car, id) {
            self.note_overflow();
        }
    }

    fn note_overflow(&mut self) {
        if let Ok(Raised::New(id)) = self
            .alarms
            .raise(AlarmKind::LogOverflow, None, self.clock_ms)
        {
            self.alarm_version = self.alarm_version.wrapping_add(1);
            self.log
                .push(self.clock_ms, EventKind::AlarmRaised, None, id);
        }
    }

    fn raise_alarm(&mut self, kind: AlarmKind, car: Option<u8>) {
        if let Ok(Raised::New(id)) = self.alarms.raise(kind, car, self.clock_ms) {
            self.alarm_version = self.alarm_version.wrapping_add(1);
            self.record(EventKind::AlarmRaised, car, id);
        }
    }

    fn clear_alarm(&mut self, kind: AlarmKind, car: Option<u8>) {
        if let Some(id) = self.alarms.clear(kind, car) {
            self.alarm_version = self.alarm_version.wrapping_add(1);
            self.record(EventKind::AlarmCleared, car, id);
        }
    }

    pub fn fail_car(&mut self, car: usize) -> Result<(), CarError> {
        let input = self.cars.get_mut(car).ok_or(CarError::NoSuchCar)?;
        if input.state.mode == CarMode::Unavailable {
            return Err(CarError::WrongState);
        }
        input.state.mode = CarMode::Unavailable;
        self.raise_alarm(AlarmKind::CarFault, u8::try_from(car).ok());
        Ok(())
    }

    pub fn recover_car(&mut self, car: usize) -> Result<(), CarError> {
        let input = self.cars.get_mut(car).ok_or(CarError::NoSuchCar)?;
        if input.state.mode != CarMode::Unavailable {
            return Err(CarError::WrongState);
        }
        input.state.mode = CarMode::Normal;
        self.clear_alarm(AlarmKind::CarFault, u8::try_from(car).ok());
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
        self.reset_cars();
        self.paused = false;
        self.forget_history();
        self.start_empty()
    }

    pub fn start_empty(&mut self) -> Option<()> {
        self.dispatcher = Some(Core::new(eta_params()?)?);
        Some(())
    }

    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, PressError> {
        self.clock_ms = now_ms;
        self.dispatcher
            .as_mut()
            .ok_or(PressError::NotRunning)?
            .press(call, now_ms)
            .map_err(PressError::Call)
    }

    pub fn reload(&mut self, now_ms: u64) -> Option<usize> {
        self.clock_ms = now_ms;
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
        let elapsed_ms =
            u32::try_from(now_ms.saturating_sub(self.last_step_ms)).unwrap_or(u32::MAX);
        self.last_step_ms = now_ms;
        self.clock_ms = now_ms;
        let mut events = StepEvents::default();
        if self.paused {
            return events;
        }
        self.drive_cars(elapsed_ms);
        let mut overflowed = false;
        let Self {
            dispatcher,
            cars,
            log,
            ..
        } = self;
        if let Some(dispatcher) = dispatcher.as_mut() {
            dispatcher.step(now_ms, cars, |event| {
                events.assigned += usize::from(matches!(event, Event::Assigned(_)));
                events.requeued += usize::from(matches!(event, Event::Requeued { .. }));
                overflowed |= log_event(log, now_ms, event);
            });
        }
        if overflowed {
            self.note_overflow();
        }
        events
    }

    fn drive_cars(&mut self, elapsed_ms: u32) {
        let Self {
            plants,
            dispatcher,
            cars,
            doors,
            assigned,
            ..
        } = self;
        let (Some(plants), Some(dispatcher)) = (plants.as_mut(), dispatcher.as_ref()) else {
            return;
        };
        let table = dispatcher.calls();
        for (index, plant) in plants.iter_mut().enumerate() {
            let id = u8::try_from(index).unwrap_or(u8::MAX);
            let targets = assigned_targets(table, id);
            let input = &mut cars[index];
            plant.step(elapsed_ms, input.state.mode, targets);
            input.state.position_mm = plant.position_mm();
            input.state.speed_mm_s = plant.speed_mm_s();
            input.state.heading = plant.heading();
            input.state.door_remaining_ms = plant.door_remaining_ms();
            input.stopped_at = plant.stopped_at();
            input.doors_open = plant.doors() == DoorState::Open;
            input.confirmations = input.stopped_at.map_or([None, None], |floor| {
                [Direction::Up, Direction::Down]
                    .map(|direction| ticket(table, id, floor, direction))
            });
            doors[index] = plant.doors();
            assigned[index] = targets;
        }
    }

    pub fn toggle_failure(&mut self, car: usize) -> bool {
        if self.is_failed(car) {
            self.recover_car(car).ok();
        } else {
            self.fail_car(car).ok();
        }
        self.is_failed(car)
    }

    #[must_use]
    pub fn car_snapshot(&self, car: usize, now_ms: u64) -> Option<CarSnapshot> {
        let state = self.cars.get(car)?.state;
        let on_floor = state.position_mm.is_multiple_of(FLOOR_HEIGHT_MM) && state.speed_mm_s == 0;
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
            doors: self.doors[car],
            floor,
            position_mm: state.position_mm,
            speed_mm_per_sec: match state.heading {
                Direction::Up => speed,
                Direction::Down => -speed,
            },
            eta_ms: None,
            up_calls: self.assigned[car].up,
            down_calls: self.assigned[car].down,
            model_time_ms: now_ms.min(JSON_SAFE_MAX),
        })
    }

    #[must_use]
    pub fn events_chunk(&self, from_index: u32) -> EventLogChunk {
        self.log.chunk(from_index)
    }

    #[must_use]
    pub fn alarm_page(&self, offset: u16) -> AlarmPage {
        self.alarms.page(offset)
    }

    #[must_use]
    pub fn critical_alarm_active(&self) -> bool {
        self.alarms
            .has_active(&[AlarmKind::CarFault, AlarmKind::FireRecall])
    }

    #[must_use]
    pub const fn alarm_version(&self) -> u32 {
        self.alarm_version
    }

    pub fn acknowledge_alarm(&mut self, id: u32) -> Result<(), AlarmError> {
        self.alarms.acknowledge(id)?;
        self.alarm_version = self.alarm_version.wrapping_add(1);
        let car = self.alarms.get(id).and_then(|alarm| alarm.car);
        self.record(EventKind::AlarmAcked, car, id);
        Ok(())
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

fn ticket(
    table: &CallTable<FLOORS>,
    car: CarId,
    floor: u8,
    direction: Direction,
) -> Option<Assignment> {
    let call = Call { floor, direction };
    let record = table.get(call)?;
    (record.car == Some(car) && matches!(record.state, CallState::Assigned | CallState::Arrived))
        .then_some(Assignment {
            call,
            id: record.id,
            car,
            attempt: record.attempt,
        })
}

fn assigned_targets(table: &CallTable<FLOORS>, car: CarId) -> Targets {
    (1..=FLOOR_COUNT).fold(Targets::default(), |mut targets, floor| {
        let bit = 1_u32 << (floor - 1);
        if ticket(table, car, floor, Direction::Up).is_some() {
            targets.up |= bit;
        }
        if ticket(table, car, floor, Direction::Down).is_some() {
            targets.down |= bit;
        }
        targets
    })
}

fn log_event(log: &mut EventLog<LOG_CAPACITY>, now_ms: u64, event: Event) -> bool {
    let (kind, car, id) = match event {
        Event::Assigned(assignment) => (EventKind::Assigned, Some(assignment.car), assignment.id),
        Event::Arrived { id, car, .. } => (EventKind::Arrived, Some(car), id),
        Event::Served { served, .. } => (EventKind::Served, Some(served.car), served.id),
        Event::Requeued { id, car, .. } => (EventKind::Requeued, Some(car), id),
        Event::Cancelled { id, .. } => (EventKind::Cancelled, None, id),
    };
    log.push(now_ms, kind, car, id)
}
