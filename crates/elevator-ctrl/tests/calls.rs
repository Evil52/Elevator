use elevator_ctrl::calls::{
    CallRecord, CallState, CallTable, Press, Rejection, Served, TransitionError,
};
use elevator_ctrl::eta::{Call, Direction};

use Direction::{Down, Up};

const FLOORS: u8 = 10;

type Table = CallTable<10>;

fn call(floor: u8, direction: Direction) -> Call {
    Call { floor, direction }
}

fn up(floor: u8) -> Call {
    call(floor, Up)
}

fn down(floor: u8) -> Call {
    call(floor, Down)
}

fn queued_calls(table: &Table) -> Vec<Call> {
    let mut out = [up(1); 32];
    let count = table.queued(&mut out);
    out[..count].to_vec()
}

#[test]
fn a_new_table_is_empty() {
    let table = Table::new();
    assert_eq!(table.get(up(3)), None);
    assert_eq!(table.get(down(3)), None);
    assert_eq!(queued_calls(&table), Vec::new());
}

#[test]
fn a_press_creates_a_queued_call_with_the_press_time() {
    let mut table = Table::new();
    assert_eq!(table.press(up(3), 1000), Ok(Press::Created(0)));
    assert_eq!(
        table.get(up(3)),
        Some(CallRecord {
            id: 0,
            state: CallState::Queued,
            registered_ms: 1000,
            car: None,
        })
    );
}

#[test]
fn up_and_down_at_one_floor_are_different_calls() {
    let mut table = Table::new();
    assert_eq!(table.press(up(5), 10), Ok(Press::Created(0)));
    assert_eq!(table.press(down(5), 20), Ok(Press::Created(1)));
    assert_eq!(table.get(up(5)).map(|r| r.registered_ms), Some(10));
    assert_eq!(table.get(down(5)).map(|r| r.registered_ms), Some(20));
}

#[test]
fn pressing_the_same_button_again_does_not_create_a_second_call() {
    let mut table = Table::new();
    assert_eq!(table.press(up(3), 1000), Ok(Press::Created(0)));
    let before = table.get(up(3));
    assert_eq!(table.press(up(3), 5000), Ok(Press::Duplicate(0)));
    assert_eq!(table.get(up(3)), before);
    assert_eq!(table.press(down(4), 6000), Ok(Press::Created(1)));
}

#[test]
fn pressing_again_while_assigned_or_arrived_changes_nothing() {
    let mut table = Table::new();
    table.press(up(3), 100).expect("создан");
    table.assign(up(3), 1).expect("назначен");
    let assigned = table.get(up(3));
    assert_eq!(table.press(up(3), 900), Ok(Press::Duplicate(0)));
    assert_eq!(table.get(up(3)), assigned);

    table.arrived(up(3)).expect("приехала");
    let arrived = table.get(up(3));
    assert_eq!(table.press(up(3), 950), Ok(Press::Duplicate(0)));
    assert_eq!(table.get(up(3)), arrived);
}

#[test]
fn wrong_calls_are_rejected_with_a_reason_and_leave_no_trace() {
    let mut table = Table::new();
    assert_eq!(table.press(up(0), 1), Err(Rejection::NoSuchFloor));
    assert_eq!(table.press(down(0), 1), Err(Rejection::NoSuchFloor));
    assert_eq!(table.press(up(FLOORS + 1), 1), Err(Rejection::NoSuchFloor));
    assert_eq!(table.press(down(255), 1), Err(Rejection::NoSuchFloor));
    assert_eq!(table.press(down(1), 1), Err(Rejection::DownFromFirst));
    assert_eq!(table.press(up(FLOORS), 1), Err(Rejection::UpFromTop));
    assert_eq!(queued_calls(&table), Vec::new());
    assert_eq!(table.press(up(1), 2), Ok(Press::Created(0)));
}

#[test]
fn the_edge_floors_accept_the_calls_that_make_sense() {
    let mut table = Table::new();
    assert_eq!(table.press(up(1), 1), Ok(Press::Created(0)));
    assert_eq!(table.press(down(FLOORS), 2), Ok(Press::Created(1)));
}

#[test]
fn a_call_goes_through_all_the_states_to_served() {
    let mut table = Table::new();
    table.press(up(3), 1000).expect("создан");

    table.assign(up(3), 2).expect("назначен");
    assert_eq!(
        table.get(up(3)),
        Some(CallRecord {
            id: 0,
            state: CallState::Assigned,
            registered_ms: 1000,
            car: Some(2),
        })
    );

    table.arrived(up(3)).expect("приехала");
    assert_eq!(table.get(up(3)).map(|r| r.state), Some(CallState::Arrived));
    assert_eq!(table.get(up(3)).and_then(|r| r.car), Some(2));

    assert_eq!(
        table.doors_opened(up(3)),
        Ok(Served {
            id: 0,
            car: 2,
            registered_ms: 1000,
        })
    );
    assert_eq!(table.get(up(3)), None);
}

#[test]
fn after_served_the_same_button_makes_a_brand_new_call() {
    let mut table = Table::new();
    table.press(up(3), 1000).expect("создан");
    table.assign(up(3), 0).expect("назначен");
    table.arrived(up(3)).expect("приехала");
    table.doors_opened(up(3)).expect("обслужен");

    assert_eq!(table.press(up(3), 9000), Ok(Press::Created(1)));
    assert_eq!(
        table.get(up(3)),
        Some(CallRecord {
            id: 1,
            state: CallState::Queued,
            registered_ms: 9000,
            car: None,
        })
    );
}

#[test]
fn transitions_out_of_order_are_refused_and_change_nothing() {
    let mut table = Table::new();
    table.press(up(3), 100).expect("создан");
    let queued = table.get(up(3));

    assert_eq!(table.arrived(up(3)), Err(TransitionError::WrongState));
    assert_eq!(table.doors_opened(up(3)), Err(TransitionError::WrongState));
    assert_eq!(table.get(up(3)), queued);

    table.assign(up(3), 0).expect("назначен");
    let assigned = table.get(up(3));
    assert_eq!(table.assign(up(3), 1), Err(TransitionError::WrongState));
    assert_eq!(table.doors_opened(up(3)), Err(TransitionError::WrongState));
    assert_eq!(table.get(up(3)), assigned);

    table.arrived(up(3)).expect("приехала");
    let arrived = table.get(up(3));
    assert_eq!(table.assign(up(3), 1), Err(TransitionError::WrongState));
    assert_eq!(table.arrived(up(3)), Err(TransitionError::WrongState));
    assert_eq!(table.get(up(3)), arrived);
}

#[test]
fn transitions_on_a_call_that_is_not_there_are_unknown() {
    let mut table = Table::new();
    assert_eq!(table.assign(up(3), 0), Err(TransitionError::UnknownCall));
    assert_eq!(table.arrived(up(3)), Err(TransitionError::UnknownCall));
    assert_eq!(table.doors_opened(up(3)), Err(TransitionError::UnknownCall));
    assert_eq!(table.assign(up(0), 0), Err(TransitionError::UnknownCall));
    assert_eq!(table.arrived(down(200)), Err(TransitionError::UnknownCall));
    assert_eq!(
        table.doors_opened(up(FLOORS + 1)),
        Err(TransitionError::UnknownCall)
    );
    assert_eq!(table.get(up(0)), None);
    assert_eq!(table.get(down(255)), None);
}

#[test]
fn a_lost_car_returns_its_calls_to_the_queue_with_the_original_time() {
    let mut table = Table::new();
    table.press(up(8), 1000).expect("создан");
    table.press(down(3), 1500).expect("создан");
    table.assign(up(8), 0).expect("назначен");
    table.assign(down(3), 1).expect("назначен");
    let other_car = table.get(down(3));

    assert_eq!(table.car_lost(0), 1);
    assert_eq!(
        table.get(up(8)),
        Some(CallRecord {
            id: 0,
            state: CallState::Queued,
            registered_ms: 1000,
            car: None,
        })
    );
    assert_eq!(table.get(down(3)), other_car);
}

#[test]
fn a_lost_car_also_returns_the_call_it_has_already_arrived_at() {
    let mut table = Table::new();
    table.press(up(8), 1000).expect("создан");
    table.assign(up(8), 0).expect("назначен");
    table.arrived(up(8)).expect("приехала");
    assert_eq!(table.car_lost(0), 1);
    assert_eq!(table.get(up(8)).map(|r| r.state), Some(CallState::Queued));
    assert_eq!(table.get(up(8)).map(|r| r.registered_ms), Some(1000));
}

#[test]
fn a_served_call_is_not_returned_by_a_later_failure() {
    let mut table = Table::new();
    table.press(up(8), 1000).expect("создан");
    table.assign(up(8), 0).expect("назначен");
    table.arrived(up(8)).expect("приехала");
    table.doors_opened(up(8)).expect("обслужен");
    assert_eq!(table.car_lost(0), 0);
    assert_eq!(table.get(up(8)), None);
    assert_eq!(queued_calls(&table), Vec::new());
}

#[test]
fn a_lost_car_with_nothing_assigned_changes_nothing() {
    let mut table = Table::new();
    table.press(up(2), 5).expect("создан");
    assert_eq!(table.car_lost(0), 0);
    assert_eq!(table.get(up(2)).map(|r| r.state), Some(CallState::Queued));
}

#[test]
fn a_requeued_call_keeps_its_place_by_the_original_time() {
    let mut table = Table::new();
    table.press(up(8), 10).expect("создан");
    table.press(down(3), 20).expect("создан");
    table.assign(up(8), 0).expect("назначен");
    assert_eq!(queued_calls(&table), vec![down(3)]);
    table.press(up(5), 30).expect("создан");
    assert_eq!(table.car_lost(0), 1);
    assert_eq!(queued_calls(&table), vec![up(8), down(3), up(5)]);
}

#[test]
fn queued_calls_with_equal_time_are_ordered_by_number() {
    let mut table = Table::new();
    table.press(down(9), 100).expect("создан");
    table.press(up(2), 100).expect("создан");
    table.press(down(4), 100).expect("создан");
    assert_eq!(queued_calls(&table), vec![down(9), up(2), down(4)]);
}

#[test]
fn only_queued_calls_are_listed() {
    let mut table = Table::new();
    table.press(up(2), 1).expect("создан");
    table.press(up(3), 2).expect("создан");
    table.press(up(4), 3).expect("создан");
    table.assign(up(2), 0).expect("назначен");
    table.assign(up(3), 1).expect("назначен");
    table.arrived(up(3)).expect("приехала");
    assert_eq!(queued_calls(&table), vec![up(4)]);
}

#[test]
fn a_short_buffer_gets_the_earliest_calls() {
    let mut table = Table::new();
    table.press(up(2), 30).expect("создан");
    table.press(up(3), 10).expect("создан");
    table.press(up(4), 20).expect("создан");
    let mut out = [up(1); 2];
    assert_eq!(table.queued(&mut out), 2);
    assert_eq!(out, [up(3), up(4)]);
    let mut none: [Call; 0] = [];
    assert_eq!(table.queued(&mut none), 0);
}

#[test]
fn fire_mode_cancels_every_call_and_numbers_keep_growing() {
    let mut table = Table::new();
    table.press(up(2), 1).expect("создан");
    table.press(up(3), 2).expect("создан");
    table.press(up(4), 3).expect("создан");
    table.assign(up(3), 0).expect("назначен");
    table.assign(up(4), 1).expect("назначен");
    table.arrived(up(4)).expect("приехала");

    assert_eq!(table.cancel_all(), 3);
    assert_eq!(table.get(up(2)), None);
    assert_eq!(table.get(up(3)), None);
    assert_eq!(table.get(up(4)), None);
    assert_eq!(queued_calls(&table), Vec::new());
    assert_eq!(table.cancel_all(), 0);
    assert_eq!(table.press(up(2), 10), Ok(Press::Created(3)));
}

#[test]
fn every_sensible_button_fits_in_the_table() {
    let mut table = Table::new();
    let mut created = 0_u32;
    for floor in 1..=FLOORS {
        for direction in [Up, Down] {
            let valid = (direction == Up && floor != FLOORS) || (direction == Down && floor != 1);
            if valid {
                assert_eq!(
                    table.press(call(floor, direction), u64::from(floor)),
                    Ok(Press::Created(created))
                );
                created += 1;
            }
        }
    }
    assert_eq!(created, 18);
    assert_eq!(queued_calls(&table).len(), 18);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelEntry {
    call: (u8, bool),
    record: CallRecord,
}

#[derive(Default)]
struct Model {
    entries: Vec<ModelEntry>,
    next_id: u32,
}

impl Model {
    fn key(call: Call) -> (u8, bool) {
        (call.floor, call.direction == Up)
    }

    fn find(&self, call: Call) -> Option<usize> {
        self.entries.iter().position(|e| e.call == Self::key(call))
    }

    fn press(&mut self, call: Call, now: u64) -> Result<Press, Rejection> {
        if call.floor == 0 || call.floor > FLOORS {
            return Err(Rejection::NoSuchFloor);
        }
        if call.direction == Down && call.floor == 1 {
            return Err(Rejection::DownFromFirst);
        }
        if call.direction == Up && call.floor == FLOORS {
            return Err(Rejection::UpFromTop);
        }
        if let Some(index) = self.find(call) {
            return Ok(Press::Duplicate(self.entries[index].record.id));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(ModelEntry {
            call: Self::key(call),
            record: CallRecord {
                id,
                state: CallState::Queued,
                registered_ms: now,
                car: None,
            },
        });
        Ok(Press::Created(id))
    }

    fn step(
        &mut self,
        call: Call,
        from: CallState,
        to: CallState,
        car: Option<u8>,
    ) -> Result<usize, TransitionError> {
        let index = self.find(call).ok_or(TransitionError::UnknownCall)?;
        let record = &mut self.entries[index].record;
        if record.state != from {
            return Err(TransitionError::WrongState);
        }
        record.state = to;
        if car.is_some() {
            record.car = car;
        }
        Ok(index)
    }

    fn doors_opened(&mut self, call: Call) -> Result<Served, TransitionError> {
        let index = self.step(call, CallState::Arrived, CallState::Arrived, None)?;
        let record = self.entries.remove(index).record;
        Ok(Served {
            id: record.id,
            car: record.car.expect("в ARRIVED кабина есть"),
            registered_ms: record.registered_ms,
        })
    }

    fn car_lost(&mut self, car: u8) -> usize {
        let mut count = 0;
        for entry in &mut self.entries {
            if entry.record.car == Some(car) {
                entry.record.state = CallState::Queued;
                entry.record.car = None;
                count += 1;
            }
        }
        count
    }

    fn queued(&self) -> Vec<Call> {
        let mut queued: Vec<&ModelEntry> = self
            .entries
            .iter()
            .filter(|e| e.record.state == CallState::Queued)
            .collect();
        queued.sort_by_key(|e| (e.record.registered_ms, e.record.id));
        queued
            .iter()
            .map(|e| Call {
                floor: e.call.0,
                direction: if e.call.1 { Up } else { Down },
            })
            .collect()
    }
}

struct Rng(u64);

impl Rng {
    fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u8) -> u8 {
        u8::try_from(self.step() % u64::from(limit)).expect("остаток меньше limit")
    }
}

#[test]
fn random_operations_agree_with_the_model() {
    for seed in 1..=40_u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut table = Table::new();
        let mut model = Model::default();
        let mut now = 0_u64;
        for step in 0..400 {
            now += u64::from(rng.below(4));
            let target = Call {
                floor: rng.below(13),
                direction: if rng.below(2) == 0 { Up } else { Down },
            };
            let car = rng.below(3);
            let what = rng.below(20);
            let context = format!("seed {seed}, шаг {step}, операция {what}, вызов {target:?}");
            match what {
                0..=7 => assert_eq!(
                    table.press(target, now),
                    model.press(target, now),
                    "{context}"
                ),
                8..=10 => assert_eq!(
                    table.assign(target, car),
                    model
                        .step(target, CallState::Queued, CallState::Assigned, Some(car))
                        .map(|_| ()),
                    "{context}"
                ),
                11..=13 => assert_eq!(
                    table.arrived(target),
                    model
                        .step(target, CallState::Assigned, CallState::Arrived, None)
                        .map(|_| ()),
                    "{context}"
                ),
                14..=16 => assert_eq!(
                    table.doors_opened(target),
                    model.doors_opened(target),
                    "{context}"
                ),
                17..=18 => assert_eq!(table.car_lost(car), model.car_lost(car), "{context}"),
                _ => {
                    let cancelled = model.entries.len();
                    model.entries.clear();
                    assert_eq!(table.cancel_all(), cancelled, "{context}");
                }
            }
            for floor in 0..=12_u8 {
                for direction in [Up, Down] {
                    let probe = Call { floor, direction };
                    let expected = model.find(probe).map(|i| model.entries[i].record);
                    assert_eq!(table.get(probe), expected, "{context}, проба {probe:?}");
                }
            }
            assert_eq!(queued_calls(&table), model.queued(), "{context}");
        }
    }
}
