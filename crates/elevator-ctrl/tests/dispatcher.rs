use elevator_ctrl::assign;
use elevator_ctrl::calls::{Assignment, CallRecord, CallState, Press, Served};
use elevator_ctrl::dispatcher::{CarInput, Dispatcher, Event};
use elevator_ctrl::eta::{
    Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams, QueuedCall, build_matrix,
    eta_ms,
};
use elevator_ctrl::motion::MotionParams;

use Direction::{Down, Up};

const HEIGHT: u32 = 3000;

fn params(aging_cap_ms: u32) -> EtaParams {
    EtaParams {
        building: Building::new(10, HEIGHT).expect("здание"),
        motion: MotionParams::new(1600, 1000, 1000).expect("движение"),
        doors: DoorTimes::new(2000, 6000, 3000).expect("двери"),
        aging_cap_ms,
    }
}

fn dispatcher<const C: usize, const K: usize>(aging_cap_ms: u32) -> Dispatcher<10, C, K> {
    Dispatcher::new(params(aging_cap_ms)).expect("диспетчер")
}

fn call(floor: u8, direction: Direction) -> Call {
    Call { floor, direction }
}

fn up(floor: u8) -> Call {
    call(floor, Up)
}

fn down(floor: u8) -> Call {
    call(floor, Down)
}

const fn pos(floor: u8) -> u32 {
    (floor as u32 - 1) * HEIGHT
}

fn idle(floor: u8) -> CarInput {
    CarInput {
        state: CarState {
            position_mm: pos(floor),
            speed_mm_s: 0,
            accel_mm_s2: 0,
            heading: Up,
            door_remaining_ms: 0,
            up_calls: 0,
            down_calls: 0,
            car_calls: 0,
            mode: CarMode::Normal,
        },
        stopped_at: None,
        doors_open: false,
        confirmations: [None; 2],
    }
}

fn stopped(floor: u8, doors_open: bool) -> CarInput {
    CarInput {
        stopped_at: Some(floor),
        doors_open,
        ..idle(floor)
    }
}

fn with_mode(input: CarInput, mode: CarMode) -> CarInput {
    CarInput {
        state: CarState {
            mode,
            ..input.state
        },
        ..input
    }
}

fn step<const C: usize, const K: usize>(
    group: &mut Dispatcher<10, C, K>,
    now_ms: u64,
    cars: &[CarInput; C],
) -> Vec<Event> {
    let mut events = Vec::new();
    let cars = std::array::from_fn(|i| {
        let mut car = cars[i];
        if let Some(floor) = car.stopped_at {
            car.confirmations = [Up, Down].map(|direction| {
                let call = call(floor, direction);
                let record = group.calls().get(call)?;
                (record.car == Some(car_id(i))).then_some(Assignment {
                    call,
                    id: record.id,
                    car: car_id(i),
                    attempt: record.attempt,
                })
            });
        }
        car
    });
    group.step(now_ms, &cars, |event| events.push(event));
    events
}

fn no_events() -> Vec<Event> {
    Vec::new()
}

fn record<const C: usize, const K: usize>(
    group: &Dispatcher<10, C, K>,
    call: Call,
) -> Option<CallRecord> {
    group.calls().get(call)
}

fn assigned_car(events: &[Event]) -> usize {
    let cars: Vec<usize> = events
        .iter()
        .filter_map(|event| match event {
            Event::Assigned(Assignment { car, .. }) => Some(usize::from(*car)),
            _ => None,
        })
        .collect();
    assert_eq!(
        cars.len(),
        1,
        "ожидалось одно назначение, события: {events:?}"
    );
    cars[0]
}

fn car_id(index: usize) -> u8 {
    u8::try_from(index).expect("номер кабины")
}

#[test]
fn a_building_that_does_not_match_the_floor_count_is_rejected() {
    let nine = EtaParams {
        building: Building::new(9, HEIGHT).expect("здание"),
        ..params(0)
    };
    assert!(Dispatcher::<10, 3, 18>::new(nine).is_none());
    assert!(Dispatcher::<10, 3, 18>::new(params(0)).is_some());
}

#[test]
fn more_cars_than_a_car_id_can_hold_are_rejected() {
    assert!(Dispatcher::<10, 257, 18>::new(params(0)).is_none());
    assert!(Dispatcher::<10, 256, 18>::new(params(0)).is_some());
}

#[test]
fn a_single_call_goes_to_the_car_with_the_smallest_eta() {
    let params = params(0);
    let cars = [idle(1), idle(6), idle(10)];
    for floor in 1..=10 {
        for direction in [Up, Down] {
            let target = call(floor, direction);
            let mut group = dispatcher::<3, 18>(0);
            if group.press(target, 0).is_err() {
                continue;
            }
            let best = (0..3)
                .min_by_key(|&index| {
                    (
                        eta_ms(&params, &cars[index].state, target).unwrap_or(u32::MAX),
                        index,
                    )
                })
                .expect("кабины есть");
            assert_eq!(
                step(&mut group, 10, &cars),
                [Event::Assigned(Assignment {
                    call: target,
                    id: 0,
                    car: car_id(best),
                    attempt: 1
                })],
                "вызов {target:?}"
            );
            let got = record(&group, target).expect("вызов в таблице");
            assert_eq!(got.state, CallState::Assigned);
            assert_eq!(got.car, Some(car_id(best)));
        }
    }
}

const WINDOW: usize = 4;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u32) -> u32 {
        u32::try_from(self.step() % u64::from(limit)).expect("остаток меньше limit")
    }

    fn floor(&mut self) -> u8 {
        u8::try_from(self.below(10) + 1).expect("этаж")
    }
}

fn random_call(rng: &mut Rng) -> Call {
    let direction = if rng.below(2) == 0 { Up } else { Down };
    call(rng.floor(), direction)
}

fn random_car(rng: &mut Rng, mode: CarMode) -> CarInput {
    let base = idle(rng.floor());
    CarInput {
        state: CarState {
            heading: if rng.below(2) == 0 { Up } else { Down },
            door_remaining_ms: if rng.below(2) == 0 {
                0
            } else {
                rng.below(8000)
            },
            up_calls: rng.below(1024),
            down_calls: rng.below(1024),
            car_calls: rng.below(1024),
            mode,
            ..base.state
        },
        ..base
    }
}

fn assert_table(
    group: &Dispatcher<10, 3, WINDOW>,
    queued: &[(Call, u64, u32)],
    owned: &[(Call, u64, u32, u8)],
    context: &str,
) {
    for &(target, registered_ms, id) in queued {
        assert_eq!(
            record(group, target),
            Some(CallRecord {
                id,
                state: CallState::Queued,
                registered_ms,
                car: None,
                attempt: 0,
            }),
            "{context}"
        );
    }
    for &(target, registered_ms, id, car) in owned {
        assert_eq!(
            record(group, target),
            Some(CallRecord {
                id,
                state: CallState::Assigned,
                registered_ms,
                car: Some(car),
                attempt: 1,
            }),
            "{context}"
        );
    }
}

#[test]
fn the_group_decision_equals_assign_over_build_matrix() {
    for seed in 1..=300_u64 {
        let mut rng = Rng::new(seed);
        let aging = [0, 20_000, 60_000][rng.below(3) as usize];
        let params = params(aging);
        let mut group = dispatcher::<3, WINDOW>(aging);
        let cars: [CarInput; 3] = std::array::from_fn(|_| {
            let mode = match rng.below(10) {
                0 => CarMode::Unavailable,
                1 => CarMode::Firefighters,
                _ => CarMode::Normal,
            };
            random_car(&mut rng, mode)
        });

        let mut now = 0_u64;
        let mut queued: Vec<(Call, u64, u32)> = Vec::new();
        let mut owned: Vec<(Call, u64, u32, u8)> = Vec::new();
        for round in 0..3 {
            for _ in 0..rng.below(8) {
                now += u64::from(rng.below(5000));
                let target = random_call(&mut rng);
                if let Ok(Press::Created(id)) = group.press(target, now) {
                    queued.push((target, now, id));
                }
            }
            now += u64::from(rng.below(40_000));

            let states: [CarState; 3] = std::array::from_fn(|index| {
                let (mut up_calls, mut down_calls) = (0_u32, 0_u32);
                for (target, _, _, car) in &owned {
                    if usize::from(*car) == index {
                        let bit = 1_u32 << (target.floor - 1);
                        match target.direction {
                            Up => up_calls |= bit,
                            Down => down_calls |= bit,
                        }
                    }
                }
                CarState {
                    up_calls,
                    down_calls,
                    ..cars[index].state
                }
            });
            let window: [QueuedCall; WINDOW] =
                std::array::from_fn(|index| match queued.get(index) {
                    Some(&(target, registered, _)) => QueuedCall {
                        call: target,
                        waited_ms: u32::try_from(now - registered).unwrap_or(u32::MAX),
                    },
                    None => QueuedCall {
                        call: up(0),
                        waited_ms: 0,
                    },
                });
            let picks = assign(&build_matrix(&params, &states, &window));
            let picked: Vec<(u8, usize)> = picks
                .iter()
                .enumerate()
                .filter_map(|(car, pick)| pick.map(|column| (car_id(car), column)))
                .collect();
            let expected: Vec<Event> = picked
                .iter()
                .map(|&(car, column)| {
                    Event::Assigned(Assignment {
                        call: queued[column].0,
                        id: queued[column].2,
                        car,
                        attempt: 1,
                    })
                })
                .collect();

            assert_eq!(
                step(&mut group, now, &cars),
                expected,
                "seed {seed}, раунд {round}, вызовы {queued:?}, закреплённые {owned:?}, кабины {cars:?}"
            );

            let taken: Vec<u32> = picked.iter().map(|&(_, column)| queued[column].2).collect();
            for &(car, column) in &picked {
                let (target, registered, id) = queued[column];
                owned.push((target, registered, id, car));
            }
            queued.retain(|(_, _, id)| !taken.contains(id));

            assert_table(
                &group,
                &queued,
                &owned,
                &format!("seed {seed}, раунд {round}"),
            );
        }
    }
}

#[test]
fn a_car_gets_at_most_one_new_call_per_step() {
    let mut group = dispatcher::<1, 18>(0);
    group.press(up(3), 0).expect("вызов");
    group.press(up(7), 0).expect("вызов");
    let first = step(&mut group, 10, &[idle(1)]);
    assert_eq!(assigned_car(&first), 0);
    let waiting = [up(3), up(7)]
        .iter()
        .filter(|target| record(&group, **target).expect("вызов").state == CallState::Queued)
        .count();
    assert_eq!(waiting, 1, "один вызов должен ждать следующего шага");

    let second = step(&mut group, 20, &[idle(1)]);
    assert_eq!(assigned_car(&second), 0);
    for target in [up(3), up(7)] {
        let got = record(&group, target).expect("вызов");
        assert_eq!((got.state, got.car), (CallState::Assigned, Some(0)));
    }
    assert_eq!(step(&mut group, 30, &[idle(1)]), no_events());
}

#[test]
fn only_the_oldest_calls_within_the_window_are_considered() {
    let mut group = dispatcher::<3, 2>(0);
    for (time, target) in [(0, up(2)), (10, up(4)), (20, up(6)), (30, up(8))] {
        group.press(target, time).expect("вызов");
    }
    let cars = [idle(1), idle(5), idle(9)];
    let events = step(&mut group, 100, &cars);
    let mut ids: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            Event::Assigned(Assignment { id, .. }) => Some(*id),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, [0, 1], "события: {events:?}");
    for target in [up(6), up(8)] {
        assert_eq!(
            record(&group, target).expect("вызов").state,
            CallState::Queued
        );
    }

    let events = step(&mut group, 200, &cars);
    let mut ids: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            Event::Assigned(Assignment { id, .. }) => Some(*id),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, [2, 3], "события: {events:?}");
}

fn old_far_and_fresh_near(aging_cap_ms: u32) -> Event {
    let mut group = dispatcher::<1, 18>(aging_cap_ms);
    group.press(up(9), 0).expect("вызов");
    group.press(up(6), 49_000).expect("вызов");
    let events = step(&mut group, 50_000, &[idle(5)]);
    assert_eq!(events.len(), 1, "события: {events:?}");
    events[0]
}

#[test]
fn the_oldest_call_wins_only_when_aging_is_on() {
    assert_eq!(
        old_far_and_fresh_near(0),
        Event::Assigned(Assignment {
            call: up(6),
            id: 1,
            car: 0,
            attempt: 1
        }),
        "без приоритета по ожиданию выигрывает ближний вызов"
    );
    assert_eq!(
        old_far_and_fresh_near(60_000),
        Event::Assigned(Assignment {
            call: up(9),
            id: 0,
            car: 0,
            attempt: 1
        }),
        "с приоритетом по ожиданию выигрывает давний вызов"
    );
}

#[test]
fn a_car_with_an_impossible_state_gets_no_call() {
    let mut group = dispatcher::<2, 18>(0);
    group.press(up(2), 0).expect("вызов");
    let mut broken = idle(1);
    broken.state.position_mm = 99_000;
    let events = step(&mut group, 10, &[broken, idle(10)]);
    assert_eq!(assigned_car(&events), 1, "события: {events:?}");
}

#[test]
fn calls_stay_queued_while_no_car_is_available_and_go_out_when_one_returns() {
    let mut group = dispatcher::<2, 18>(0);
    group.press(down(7), 50).expect("вызов");
    let dead = [
        with_mode(idle(1), CarMode::Unavailable),
        with_mode(idle(10), CarMode::Firefighters),
    ];
    assert_eq!(step(&mut group, 300, &dead), no_events());
    assert_eq!(
        record(&group, down(7)),
        Some(CallRecord {
            id: 0,
            state: CallState::Queued,
            registered_ms: 50,
            car: None,
            attempt: 0,
        })
    );

    let events = step(&mut group, 400, &[idle(1), dead[1]]);
    assert_eq!(
        events,
        [Event::Assigned(Assignment {
            call: down(7),
            id: 0,
            car: 0,
            attempt: 1
        })]
    );
}

#[test]
fn a_call_is_served_after_the_car_arrives_and_the_doors_open() {
    let mut group = dispatcher::<1, 18>(0);
    assert_eq!(group.press(up(4), 100), Ok(Press::Created(0)));
    let target = up(4);
    assert_eq!(
        step(&mut group, 200, &[idle(1)]),
        [Event::Assigned(Assignment {
            call: target,
            id: 0,
            car: 0,
            attempt: 1
        })]
    );

    assert_eq!(
        step(&mut group, 300, &[stopped(4, false)]),
        [Event::Arrived {
            call: target,
            id: 0,
            car: 0
        }]
    );
    assert_eq!(
        record(&group, target).expect("вызов").state,
        CallState::Arrived
    );

    assert_eq!(
        step(&mut group, 400, &[stopped(4, true)]),
        [Event::Served {
            call: target,
            served: Served {
                id: 0,
                car: 0,
                registered_ms: 100
            }
        }]
    );
    assert_eq!(record(&group, target), None);
    assert_eq!(step(&mut group, 500, &[stopped(4, true)]), no_events());
}

#[test]
fn doors_that_are_already_open_give_arrived_and_served_in_one_step() {
    let mut group = dispatcher::<1, 18>(0);
    group.press(down(6), 10).expect("вызов");
    let target = down(6);
    let first = step(&mut group, 20, &[idle(8)]);
    assert_eq!(assigned_car(&first), 0);
    assert_eq!(
        step(&mut group, 30, &[stopped(6, true)]),
        [
            Event::Arrived {
                call: target,
                id: 0,
                car: 0
            },
            Event::Served {
                call: target,
                served: Served {
                    id: 0,
                    car: 0,
                    registered_ms: 10
                }
            }
        ]
    );
    assert_eq!(record(&group, target), None);
}

#[test]
fn a_stop_at_another_floor_or_by_another_car_does_not_arrive_the_call() {
    let mut group = dispatcher::<2, 18>(0);
    group.press(up(5), 0).expect("вызов");
    let events = step(&mut group, 10, &[idle(1), idle(10)]);
    let owner = assigned_car(&events);
    let other = 1 - owner;

    let mut cars = [idle(1), idle(10)];
    cars[owner] = stopped(4, true);
    cars[other] = stopped(5, true);
    assert_eq!(step(&mut group, 20, &cars), no_events());
    let got = record(&group, up(5)).expect("вызов");
    assert_eq!(
        (got.state, got.car),
        (CallState::Assigned, Some(car_id(owner)))
    );

    cars[owner] = stopped(5, false);
    assert_eq!(
        step(&mut group, 30, &cars),
        [Event::Arrived {
            call: up(5),
            id: 0,
            car: car_id(owner)
        }]
    );
}

fn assigned_up_4(group: &mut Dispatcher<10, 2, 18>, arrived: bool) -> usize {
    group.press(up(4), 100).expect("вызов");
    let events = step(group, 200, &[idle(1), idle(10)]);
    let owner = assigned_car(&events);
    if arrived {
        let mut cars = [idle(1), idle(10)];
        cars[owner] = stopped(4, false);
        assert_eq!(
            step(group, 300, &cars),
            [Event::Arrived {
                call: up(4),
                id: 0,
                car: car_id(owner)
            }]
        );
    }
    owner
}

#[test]
fn an_arrived_call_does_not_add_another_stop_to_the_eta_of_its_car() {
    let mut group = dispatcher::<2, 18>(0);
    assert_eq!(assigned_up_4(&mut group, false), 0);
    let cars = [stopped(4, false), idle(1)];
    assert_eq!(
        step(&mut group, 300, &cars),
        [Event::Arrived {
            call: up(4),
            id: 0,
            car: 0
        }]
    );

    let params = params(0);
    let with_stop = CarState {
        up_calls: 1 << 3,
        ..cars[0].state
    };
    let target = up(6);
    let far = eta_ms(&params, &cars[1].state, target).expect("eta");
    assert!(eta_ms(&params, &cars[0].state, target).expect("eta") < far);
    assert!(eta_ms(&params, &with_stop, target).expect("eta") > far);

    group.press(target, 400).expect("вызов");
    assert_eq!(
        step(&mut group, 500, &cars),
        [Event::Assigned(Assignment {
            call: target,
            id: 1,
            car: 0,
            attempt: 1
        })]
    );
}

#[test]
fn a_car_that_leaves_the_normal_mode_returns_its_call_and_another_car_takes_it() {
    for mode in [CarMode::Unavailable, CarMode::Firefighters] {
        for arrived in [false, true] {
            let mut group = dispatcher::<2, 18>(0);
            let owner = assigned_up_4(&mut group, arrived);
            let other = 1 - owner;

            let mut cars = [idle(1), idle(10)];
            cars[owner] = with_mode(cars[owner], mode);
            assert_eq!(
                step(&mut group, 400, &cars),
                [
                    Event::Requeued {
                        call: up(4),
                        id: 0,
                        car: car_id(owner)
                    },
                    Event::Assigned(Assignment {
                        call: up(4),
                        id: 0,
                        car: car_id(other),
                        attempt: 2
                    })
                ],
                "режим {mode:?}, дошла до этажа: {arrived}"
            );
            assert_eq!(
                record(&group, up(4)),
                Some(CallRecord {
                    id: 0,
                    state: CallState::Assigned,
                    registered_ms: 100,
                    car: Some(car_id(other)),
                    attempt: 2,
                })
            );
        }
    }
}

#[test]
fn a_served_call_is_not_returned_to_the_queue_when_the_car_fails_afterwards() {
    let mut group = dispatcher::<2, 18>(0);
    let owner = assigned_up_4(&mut group, true);
    let mut cars = [idle(1), idle(10)];
    cars[owner] = stopped(4, true);
    let events = step(&mut group, 400, &cars);
    assert_eq!(
        events,
        [Event::Served {
            call: up(4),
            served: Served {
                id: 0,
                car: car_id(owner),
                registered_ms: 100
            }
        }]
    );

    cars[owner] = with_mode(idle(4), CarMode::Unavailable);
    assert_eq!(step(&mut group, 500, &cars), no_events());
    assert_eq!(record(&group, up(4)), None);
}

#[test]
fn fire_recall_cancels_every_call_and_assigns_nothing() {
    let mut group = dispatcher::<3, 18>(0);
    for (time, target) in [(100, up(3)), (110, up(5)), (120, down(8))] {
        group.press(target, time).expect("вызов");
    }
    let events = step(&mut group, 200, &[idle(1), idle(5), idle(9)]);
    assert_eq!(events.len(), 3, "события: {events:?}");

    let mut cars = [idle(1), idle(5), idle(9)];
    for event in &events {
        let Event::Assigned(Assignment { call, car, .. }) = event else {
            panic!("ожидалось назначение, получено {event:?}");
        };
        cars[usize::from(*car)] = stopped(call.floor, false);
    }
    assert_eq!(step(&mut group, 300, &cars).len(), 3);

    group.press(down(6), 310).expect("вызов");

    cars[0] = with_mode(cars[0], CarMode::FireRecall);
    let events = step(&mut group, 400, &cars);
    let mut cancelled: Vec<(u32, Call)> = events
        .iter()
        .map(|event| match event {
            Event::Cancelled { call, id } => (*id, *call),
            other => panic!("при пожаре допустимы только отмены, получено {other:?}"),
        })
        .collect();
    cancelled.sort_by_key(|(id, _)| *id);
    assert_eq!(
        cancelled,
        [(0, up(3)), (1, up(5)), (2, down(8)), (3, down(6))]
    );
    for target in [up(3), up(5), down(8), down(6)] {
        assert_eq!(record(&group, target), None, "вызов {target:?}");
    }

    group.press(up(2), 410).expect("вызов");
    assert_eq!(
        step(&mut group, 420, &cars),
        [Event::Cancelled { call: up(2), id: 4 }]
    );

    let calm = [idle(1), idle(5), idle(9)];
    group.press(up(2), 510).expect("вызов");
    let events = step(&mut group, 520, &calm);
    assert_eq!(assigned_car(&events), 0, "события: {events:?}");
    assert_eq!(record(&group, up(2)).expect("вызов").id, 5);
}

fn random_mode(rng: &mut Rng) -> CarMode {
    match rng.below(100) {
        0..=74 => CarMode::Normal,
        75..=87 => CarMode::Unavailable,
        88..=96 => CarMode::Firefighters,
        _ => CarMode::FireRecall,
    }
}

fn run_trace(seed: u64) -> Vec<Vec<Event>> {
    let mut rng = Rng::new(seed);
    let mut group = dispatcher::<3, WINDOW>(30_000);
    let mut now = 0_u64;
    let mut log = Vec::new();
    for _ in 0..60 {
        now += 1 + u64::from(rng.below(3000));
        for _ in 0..rng.below(3) {
            let _ = group.press(random_call(&mut rng), now);
        }
        let modes: [CarMode; 3] = std::array::from_fn(|_| random_mode(&mut rng));
        let cars: [CarInput; 3] = std::array::from_fn(|index| {
            let mut floors = Vec::new();
            for floor in 1..=10 {
                for direction in [Up, Down] {
                    let owner = record(&group, call(floor, direction)).and_then(|got| got.car);
                    if owner == Some(car_id(index)) {
                        floors.push(floor);
                    }
                }
            }
            let input = if floors.is_empty() || rng.below(2) == 0 {
                idle(rng.floor())
            } else {
                let pick = floors[rng.below(u32::try_from(floors.len()).expect("мало")) as usize];
                stopped(pick, rng.below(2) == 0)
            };
            with_mode(input, modes[index])
        });

        let events = step(&mut group, now, &cars);

        for floor in 1..=10 {
            for direction in [Up, Down] {
                if let Some(got) = record(&group, call(floor, direction))
                    && let Some(owner) = got.car
                {
                    assert!(
                        modes[usize::from(owner)].accepts_assignments(),
                        "вызов {got:?} остался у кабины {owner} в режиме {:?}",
                        modes[usize::from(owner)]
                    );
                }
            }
        }
        let mut assigned_cars = Vec::new();
        for event in &events {
            match *event {
                Event::Assigned(Assignment { call, id, car, .. }) => {
                    let got = record(&group, call).expect("назначенный вызов в таблице");
                    assert_eq!(
                        (got.id, got.state, got.car),
                        (id, CallState::Assigned, Some(car))
                    );
                    assigned_cars.push(car);
                }
                Event::Served { call, .. } => assert_eq!(record(&group, call), None),
                _ => {}
            }
        }
        let count = assigned_cars.len();
        assigned_cars.sort_unstable();
        assigned_cars.dedup();
        assert_eq!(
            assigned_cars.len(),
            count,
            "кабина получила два вызова: {events:?}"
        );
        log.push(events);
    }
    log
}

#[test]
fn random_traces_keep_the_invariants_and_replay_identically() {
    let mut totals = [0_usize; 5];
    for seed in 1..=40_u64 {
        let first = run_trace(seed);
        assert_eq!(
            first,
            run_trace(seed),
            "seed {seed}: повтор дал другие события"
        );
        for event in first.iter().flatten() {
            let slot = match event {
                Event::Assigned(_) => 0,
                Event::Arrived { .. } => 1,
                Event::Served { .. } => 2,
                Event::Requeued { .. } => 3,
                Event::Cancelled { .. } => 4,
            };
            totals[slot] += 1;
        }
    }
    assert!(
        totals.iter().all(|&count| count > 0),
        "не все виды событий встретились (назначено, прибыло, обслужено, возвращено, отменено): {totals:?}"
    );
}

#[test]
fn a_late_stop_report_cannot_serve_a_retry_on_the_same_car() {
    let mut group = dispatcher::<1, 2>(0);
    let target = up(4);
    group.press(target, 10).unwrap();
    let Event::Assigned(first) = step(&mut group, 20, &[idle(1)])[0] else {
        panic!()
    };
    step(&mut group, 30, &[with_mode(idle(1), CarMode::Unavailable)]);
    let Event::Assigned(second) = step(&mut group, 40, &[idle(1)])[0] else {
        panic!()
    };
    let mut car = stopped(4, true);
    car.confirmations = [Some(first), None];
    group.step(50, &[car], |event| {
        panic!("stale report produced {event:?}")
    });
    assert_eq!(record(&group, target).unwrap().state, CallState::Assigned);
    car.confirmations = [Some(second), None];
    let mut events = Vec::new();
    group.step(60, &[car], |event| events.push(event));
    assert!(matches!(
        events.as_slice(),
        [Event::Arrived { .. }, Event::Served { .. }]
    ));
}
