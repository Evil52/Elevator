use elevator_ctrl::{
    Cell,
    calls::Assignment,
    dispatcher::{CarInput, Dispatcher, Event},
    eta::{Call, CarMode, CarState, Direction, QueuedCall},
    motion::{EmergencyStop, MotionParams, RestTrip},
};
use serde_json::{Value, json};

use super::{CAR_IDS, FLOOR_HEIGHT_MM, OPEN_MS, call_event, params, profile};

const DWELL_MS: u32 = 5000;
const INITIAL_HOLD_MS: u32 = 2000;
const CALLS: [Call; 2] = [
    Call {
        floor: 3,
        direction: Direction::Down,
    },
    Call {
        floor: 8,
        direction: Direction::Up,
    },
];
type Group = Dispatcher<10, 3, 2>;

fn height(floor: u8) -> u32 {
    u32::from(floor - 1) * FLOOR_HEIGHT_MM
}

#[derive(Default)]
struct Trace {
    events: Vec<Value>,
    motions: Vec<Value>,
    doors: Vec<Value>,
}

impl Trace {
    fn door(&mut self, car: &str, floor: u8, start: u32, end: u32, from: u16, to: u16) {
        self.doors
            .push(json!({"car":car, "floor":floor, "t0Ms":start, "t1Ms":end,
            "fromPermille":from, "toPermille":to}));
    }
}

#[derive(Clone, Copy)]
struct Run {
    start: u32,
    from: u32,
    floor: u8,
    trip: RestTrip,
}

impl Run {
    fn sign(self) -> i64 {
        if height(self.floor) >= self.from {
            1
        } else {
            -1
        }
    }

    fn sample(self, now: u32) -> (u32, u32) {
        let (distance, speed) = self.trip.sample(now - self.start);
        let position = if self.sign() > 0 {
            self.from + distance
        } else {
            self.from - distance
        };
        (position, speed)
    }

    fn record(self, car: &str, now: u32, trace: &mut Trace) {
        trace
            .motions
            .push(json!({"car":car, "t0Ms":self.start, "t1Ms":now,
            "fromMm":self.from, "toMm":self.sample(now).0,
            "profile":profile(&self.trip, now-self.start, self.sign())}));
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Idle,
    Travel(Run),
    Opening(u32),
    Holding(u32),
    Closing(u32),
    Braking {
        start: u32,
        from: u32,
        brake: EmergencyStop,
    },
    Locked,
}

struct Car {
    name: &'static str,
    phase: Phase,
    position: u32,
    heading: Direction,
    pickup: Option<Assignment>,
    passenger: Option<u8>,
}

impl Car {
    fn new(index: usize, floor: u8) -> Self {
        Self {
            name: CAR_IDS[index],
            phase: Phase::Idle,
            position: height(floor),
            heading: Direction::Up,
            pickup: None,
            passenger: None,
        }
    }

    fn floor(&self) -> u8 {
        u8::try_from(self.position / FLOOR_HEIGHT_MM + 1).unwrap()
    }

    fn signed(&self, magnitude: u64) -> i64 {
        let value = i64::try_from(magnitude).unwrap();
        match self.heading {
            Direction::Up => value,
            Direction::Down => -value,
        }
    }

    fn input(&self, now: u32) -> CarInput {
        let (position_mm, speed_mm_s) = match self.phase {
            Phase::Travel(run) => run.sample(now),
            Phase::Braking { start, from, brake } => {
                let (dx, v) = brake.sample(u64::from(now - start));
                (u32::try_from(i64::from(from) + self.signed(dx)).unwrap(), v)
            }
            _ => (self.position, 0),
        };
        let mut input = CarInput {
            state: CarState {
                position_mm,
                speed_mm_s,
                accel_mm_s2: 0,
                heading: self.heading,
                door_remaining_ms: 0,
                up_calls: 0,
                down_calls: 0,
                car_calls: self.passenger.map_or(0, |floor| 1 << (floor - 1)),
                mode: CarMode::Normal,
            },
            stopped_at: None,
            doors_open: false,
            confirmations: [self.pickup, None],
        };
        match self.phase {
            Phase::Opening(end) => {
                input.stopped_at = Some(self.floor());
                input.state.door_remaining_ms = end - now + DWELL_MS + OPEN_MS;
            }
            Phase::Holding(end) => {
                input.stopped_at = Some(self.floor());
                input.doors_open = true;
                input.state.door_remaining_ms = end - now + OPEN_MS;
            }
            Phase::Closing(end) => input.state.door_remaining_ms = end - now,
            Phase::Braking { .. } | Phase::Locked => input.state.mode = CarMode::Unavailable,
            _ => {}
        }
        input
    }

    fn advance(&mut self, now: u32, landing_locked: bool, trace: &mut Trace) -> bool {
        let car = self.name;
        match self.phase {
            Phase::Travel(run) => {
                let (position, speed) = run.sample(now);
                if !landing_locked {
                    run.record(car, now, trace);
                    trace
                        .events
                        .push(json!({"tMs":now, "type":"ALARM", "car":car,
                        "code":"B1", "reason":"LANDING_LOCK_LOST"}));
                    if speed == 0 {
                        self.position = position;
                        self.phase = Phase::Locked;
                        trace
                            .events
                            .push(json!({"tMs":now, "type":"LOCKED", "car":car}));
                        return true;
                    }
                    self.phase = Phase::Braking {
                        start: now,
                        from: position,
                        brake: EmergencyStop::new(speed, 3000).unwrap(),
                    };
                } else if now - run.start == run.trip.duration_ms() {
                    run.record(car, now, trace);
                    self.position = position;
                    if self.passenger == Some(run.floor) {
                        self.passenger = None;
                        trace.events.push(
                            json!({"tMs":now, "type":"DESTINATION", "car":car, "floor":run.floor}),
                        );
                    }
                    trace.door(car, run.floor, now, now + OPEN_MS, 0, 1000);
                    self.phase = Phase::Opening(now + OPEN_MS);
                } else {
                    return false;
                }
            }
            Phase::Braking { start, from, brake } => {
                let (distance, speed) = brake.sample(u64::from(now - start));
                if speed != 0 {
                    return false;
                }
                let displacement = self.signed(distance);
                self.position = u32::try_from(i64::from(from) + displacement).unwrap();
                trace
                    .motions
                    .push(json!({"car":car, "t0Ms":start, "t1Ms":now,
                    "fromMm":from, "toMm":self.position,
                    "profile":[{"dtMs":now-start, "xMm":0, "dxMm":displacement,
                        "v0MmPerSec":self.signed(u64::from(brake.sample(0).1)), "v1MmPerSec":0}]}));
                trace
                    .events
                    .push(json!({"tMs":now, "type":"LOCKED", "car":car}));
                self.phase = Phase::Locked;
            }
            Phase::Opening(end) if now == end => self.phase = Phase::Holding(end + DWELL_MS),
            Phase::Holding(end) if now == end => {
                trace.events.push(
                    json!({"tMs":now, "type":"DOOR_CLOSING", "car":car, "floor":self.floor()}),
                );
                trace.door(car, self.floor(), now, now + OPEN_MS, 1000, 0);
                self.phase = Phase::Closing(now + OPEN_MS);
            }
            Phase::Closing(end) if now == end => self.phase = Phase::Idle,
            _ => return false,
        }
        true
    }

    fn depart(&mut self, motion: MotionParams, now: u32, trace: &mut Trace) {
        if !matches!(self.phase, Phase::Idle) {
            return;
        }
        let Some(floor) = self
            .passenger
            .or_else(|| self.pickup.map(|ticket| ticket.call.floor))
        else {
            return;
        };
        self.heading = if height(floor) >= self.position {
            Direction::Up
        } else {
            Direction::Down
        };
        let trip = motion
            .rest_trip(self.position.abs_diff(height(floor)))
            .unwrap();
        self.phase = Phase::Travel(Run {
            start: now,
            from: self.position,
            floor,
            trip,
        });
        trace
            .events
            .push(json!({"tMs":now, "type":"DEPART", "car":self.name, "floor":floor}));
    }
}

fn plan(
    now: u32,
    cars: &[Car; 3],
    calls: &[QueuedCall],
    matrix: &[[Cell; 2]; 3],
    picks: &[Option<usize>; 3],
) -> Value {
    let mut greedy = [None; 3];
    for (column, target) in calls.iter().enumerate() {
        let nearest = (0..3)
            .filter(|&i| {
                greedy[i].is_none()
                    && matrix[i][column].allowed
                    && matches!(cars[i].phase, Phase::Idle)
                    && cars[i].pickup.is_none()
                    && cars[i].passenger.is_none()
            })
            .min_by_key(|&i| cars[i].position.abs_diff(height(target.call.floor)));
        if let Some(i) = nearest {
            greedy[i] = Some(column);
        }
    }
    let result = |choices: &[Option<usize>; 3]| {
        let mut pairs = Vec::new();
        let mut total = 0;
        for (i, choice) in choices.iter().enumerate() {
            if let Some(column) = *choice {
                let cost = matrix[i][column].eta_ms;
                total += cost;
                pairs.push(json!({"car":CAR_IDS[i], "column":column, "etaMs":cost}));
            }
        }
        json!({"pairs":pairs, "totalEtaMs":total})
    };
    json!({"type":"DISPATCH", "tMs":now, "rows":CAR_IDS,
        "columns":calls.iter().map(|q| format!("H{:02}", q.call.floor)).collect::<Vec<_>>(),
        "matrix":matrix.map(|row| row[..calls.len()].iter().map(|c| c.allowed.then_some(c.eta_ms)).collect::<Vec<_>>()),
        "allowed":matrix.map(|row| row[..calls.len()].iter().map(|c| u8::from(c.allowed)).collect::<Vec<_>>()),
        "optimal":result(picks), "greedy":result(&greedy)})
}

fn dispatch(group: &mut Group, cars: &mut [Car; 3], now: u32, trace: &mut Trace) {
    let inputs = cars.each_ref().map(|car| car.input(now));
    let mut events = Vec::new();
    let mut decision = None;
    group.step_with_plan(
        u64::from(now),
        &inputs,
        |event| events.push(event),
        |calls, matrix, picks| decision = Some(plan(now, cars, calls, matrix, picks)),
    );
    for event in events {
        let transition = match event {
            Event::Assigned(ticket) => {
                if let Some(dispatch) = decision.take() {
                    trace.events.push(dispatch);
                }
                let car = &mut cars[usize::from(ticket.car)];
                assert!(
                    car.pickup.replace(ticket).is_none(),
                    "two pending pickups on {}",
                    car.name
                );
                let registered = group.calls().get(ticket.call).unwrap().registered_ms;
                call_event(
                    now,
                    ticket.call.floor,
                    "ASSIGNED",
                    registered,
                    Some(ticket.car),
                )
            }
            Event::Requeued { call, car, .. } => {
                cars[usize::from(car)].pickup = None;
                let mut e = call_event(
                    now,
                    call.floor,
                    "QUEUED",
                    group.calls().get(call).unwrap().registered_ms,
                    None,
                );
                e["type"] = json!("REQUEUE");
                e
            }
            Event::Arrived { call, car, .. } => call_event(
                now,
                call.floor,
                "ARRIVED",
                group.calls().get(call).unwrap().registered_ms,
                Some(car),
            ),
            Event::Served { call, served } => {
                let car = &mut cars[usize::from(served.car)];
                car.pickup = None;
                car.passenger = Some(match call.direction {
                    Direction::Up => 10,
                    Direction::Down => 1,
                });
                call_event(
                    now,
                    call.floor,
                    "SERVED",
                    served.registered_ms,
                    Some(served.car),
                )
            }
            Event::Cancelled { .. } => unreachable!("no fire recall in B1"),
        };
        trace.events.push(transition);
    }
    if let Some(dispatch) = decision {
        trace.events.push(dispatch);
    }
}

pub(super) fn recording() -> Value {
    let params = params();
    let mut group = Group::new(params).unwrap();
    let mut trace = Trace::default();
    let mut cars = [Car::new(0, 4), Car::new(1, 1), Car::new(2, 10)];
    let initial = json!({"A":cars[0].position, "B":cars[1].position, "C":cars[2].position});
    cars[2].phase = Phase::Holding(INITIAL_HOLD_MS);
    trace.door("C", 10, 0, INITIAL_HOLD_MS, 1000, 1000);
    trace.events.push(json!({"type":"START", "tMs":0}));
    let calls = CALLS.map(|call| {
        group.press(call, 0).unwrap();
        let registered = group.calls().get(call).unwrap().registered_ms;
        trace
            .events
            .push(call_event(0, call.floor, "QUEUED", registered, None));
        json!({"id":format!("H{:02}",call.floor), "floor":call.floor, "registeredMs":registered,
            "direction":match call.direction {Direction::Up => "UP", Direction::Down => "DOWN"}})
    });
    for now in 0..=120_000 {
        let changes = cars.each_mut().map(|car| {
            let lost_lock = car.name == "A"
                && matches!(car.phase, Phase::Travel(run) if run.sample(now).0 >= height(6));
            car.advance(now, !lost_lock, &mut trace)
        });
        if now == 0 || changes.contains(&true) {
            dispatch(&mut group, &mut cars, now, &mut trace);
            for car in &mut cars {
                car.depart(params.motion, now, &mut trace);
            }
            if matches!(cars[0].phase, Phase::Locked)
                && CALLS.iter().all(|call| group.calls().get(*call).is_none())
                && cars[1..].iter().all(|car| matches!(car.phase, Phase::Idle))
            {
                trace
                    .events
                    .push(json!({"tMs":now, "type":"DIAGNOSTICS", "car":"A"}));
                let end = now + 6000;
                trace.events.push(json!({"tMs":end, "type":"RESET"}));
                trace.motions.sort_by_key(|m| m["t0Ms"].as_u64().unwrap());
                return json!({"schemaVersion":1, "source":"rust-sil", "initial":initial, "calls":calls,
                    "durationMs":end, "events":trace.events, "motions":trace.motions, "doors":trace.doors});
            }
        }
    }
    panic!("B1 did not complete")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_lock_during_descent_keeps_the_direction_until_the_stop() {
        let mut car = Car::new(2, 8);
        let mut trace = Trace::default();
        car.passenger = Some(4);
        car.depart(params().motion, 0, &mut trace);
        assert!(car.advance(3250, false, &mut trace));
        let moving = car.input(3750).state;
        assert_eq!(moving.mode, CarMode::Unavailable);
        assert_eq!((moving.position_mm, moving.speed_mm_s), (13_875, 1500));
        assert!(!car.advance(4249, true, &mut trace));
        assert!(car.advance(4250, true, &mut trace));
        assert!(matches!(car.phase, Phase::Locked));
        assert_eq!(car.position, 13_500);
        assert_eq!(trace.motions[1]["profile"][0]["dxMm"], -1500);
        assert_eq!(trace.motions[1]["profile"][0]["v0MmPerSec"], -3000);
    }
}
