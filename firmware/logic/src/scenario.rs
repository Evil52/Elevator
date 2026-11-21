use elevator_ctrl::dispatcher::{CarInput, Dispatcher, Event};
use elevator_ctrl::eta::{Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams};
use elevator_ctrl::motion::MotionParams;

pub const FLOORS: usize = 10;
pub const CARS: usize = 3;
pub const CALLS: usize = 18;

const FLOOR_HEIGHT_MM: u32 = 3000;
const CAR_FLOORS: [u32; CARS] = [1, 5, 10];

type Core = Dispatcher<FLOORS, CARS, CALLS>;

pub struct Scenario {
    dispatcher: Option<Core>,
    cars: [CarInput; CARS],
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
        }
    }

    pub fn restart(&mut self, now_ms: u64) -> Option<usize> {
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

    pub fn step(&mut self, now_ms: u64) -> usize {
        let mut assigned = 0;
        if let Some(dispatcher) = self.dispatcher.as_mut() {
            dispatcher.step(now_ms, &self.cars, |event| {
                assigned += usize::from(matches!(event, Event::Assigned(_)));
            });
        }
        assigned
    }
}

impl Default for Scenario {
    fn default() -> Self {
        Self::new()
    }
}
