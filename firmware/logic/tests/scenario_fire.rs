#![allow(clippy::expect_used)]

use elevator_ctrl::eta::{Call, Direction};
use elevator_fw_logic::scenario::{CARS, CarError, Scenario};
use elevator_wire::protocol::{
    Alarm, AlarmKind, AlarmState, CarMode, CarSnapshot, DoorState, Event, EventKind,
};

const STEP_MS: u64 = 10;

fn call(floor: u8, direction: Direction) -> Call {
    Call { floor, direction }
}

fn events_of(scenario: &Scenario) -> Vec<Event> {
    let mut all = Vec::new();
    let mut from = 0;
    loop {
        let chunk = scenario.events_chunk(from);
        let got: Vec<Event> = chunk.events.iter().flatten().copied().collect();
        if got.is_empty() {
            return all;
        }
        from = got.last().map_or(from, |event| event.index + 1);
        all.extend(got);
    }
}

fn count(scenario: &Scenario, kind: EventKind) -> usize {
    events_of(scenario)
        .iter()
        .filter(|event| event.kind == kind)
        .count()
}

fn advance(scenario: &mut Scenario, now: &mut u64, millis: u64) {
    let end = *now + millis;
    while *now < end {
        *now += STEP_MS;
        scenario.step(*now);
    }
}

fn running_until(
    scenario: &mut Scenario,
    now: &mut u64,
    limit_ms: u64,
    done: impl Fn(&Scenario) -> bool,
) -> bool {
    let end = *now + limit_ms;
    while !done(scenario) {
        if *now >= end {
            return false;
        }
        *now += STEP_MS;
        scenario.step(*now);
    }
    true
}

fn started() -> Scenario {
    let mut scenario = Scenario::new();
    assert!(scenario.reset().is_some());
    scenario
}

fn snapshot(scenario: &Scenario, car: usize) -> CarSnapshot {
    scenario.car_snapshot(car, 0).expect("snapshot")
}

fn all_home(scenario: &Scenario) -> bool {
    (0..CARS).all(|car| {
        let s = snapshot(scenario, car);
        s.floor == Some(1) && s.doors == DoorState::Open
    })
}

fn fire_alarm(scenario: &Scenario) -> Option<Alarm> {
    scenario
        .alarm_page(0)
        .items
        .into_iter()
        .flatten()
        .find(|alarm| alarm.kind == AlarmKind::FireRecall)
}

#[test]
fn the_recall_is_accepted_once_and_cleared_once() {
    let mut scenario = started();
    assert!(!scenario.is_fire());
    assert_eq!(scenario.clear_fire(), Err(CarError::WrongState));
    assert_eq!(scenario.fire_recall(), Ok(()));
    assert!(scenario.is_fire());
    assert_eq!(scenario.fire_recall(), Err(CarError::WrongState));
    assert_eq!(scenario.clear_fire(), Ok(()));
    assert!(!scenario.is_fire());
    assert_eq!(scenario.clear_fire(), Err(CarError::WrongState));
}

#[test]
fn the_recall_puts_every_working_car_in_recall_mode_and_raises_a_critical_alarm() {
    let mut scenario = started();
    assert!(!scenario.critical_alarm_active());
    scenario.fire_recall().expect("recall");
    for car in 0..CARS {
        assert_eq!(snapshot(&scenario, car).mode, CarMode::FireRecall);
    }
    let alarm = fire_alarm(&scenario).expect("alarm");
    assert_eq!(alarm.state, AlarmState::Active);
    assert_eq!(alarm.car, None);
    assert!(scenario.critical_alarm_active());
}

#[test]
fn every_car_goes_to_the_main_floor_and_stands_with_open_doors() {
    let mut scenario = started();
    let mut now = 0;
    scenario.fire_recall().expect("recall");
    assert!(running_until(&mut scenario, &mut now, 120_000, all_home));
    advance(&mut scenario, &mut now, 60_000);
    assert!(all_home(&scenario));
    for car in 0..CARS {
        assert_eq!(snapshot(&scenario, car).speed_mm_per_sec, 0);
    }
}

#[test]
fn waiting_calls_are_cancelled_and_new_ones_are_not_served() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(7, Direction::Up), now).expect("press");
    scenario
        .press(call(9, Direction::Down), now)
        .expect("press");
    advance(&mut scenario, &mut now, 500);
    scenario.fire_recall().expect("recall");
    advance(&mut scenario, &mut now, 20);
    assert_eq!(count(&scenario, EventKind::Cancelled), 2);
    scenario.press(call(4, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 60_000);
    assert_eq!(count(&scenario, EventKind::Cancelled), 3);
    assert_eq!(count(&scenario, EventKind::Served), 0);
    assert!(all_home(&scenario));
}

#[test]
fn clearing_the_recall_returns_the_cars_to_service() {
    let mut scenario = started();
    let mut now = 0;
    scenario.fire_recall().expect("recall");
    assert!(running_until(&mut scenario, &mut now, 120_000, all_home));
    scenario.clear_fire().expect("clear");
    for car in 0..CARS {
        assert_eq!(snapshot(&scenario, car).mode, CarMode::Normal);
    }
    assert_eq!(
        fire_alarm(&scenario).expect("alarm").state,
        AlarmState::Cleared
    );
    assert!(!scenario.critical_alarm_active());
    scenario.press(call(6, Direction::Up), now).expect("press");
    assert!(running_until(&mut scenario, &mut now, 120_000, |s| {
        count(s, EventKind::Served) == 1
    }));
}

#[test]
fn a_car_failed_before_the_recall_stays_failed_and_does_not_move() {
    let mut scenario = started();
    let mut now = 0;
    scenario.fail_car(2).expect("fail");
    scenario.fire_recall().expect("recall");
    assert_eq!(snapshot(&scenario, 2).mode, CarMode::Unavailable);
    advance(&mut scenario, &mut now, 120_000);
    assert_eq!(snapshot(&scenario, 2).floor, Some(10));
    assert!((0..2).all(|car| snapshot(&scenario, car).floor == Some(1)));
}

#[test]
fn a_car_recovered_during_the_recall_joins_it() {
    let mut scenario = started();
    let mut now = 0;
    scenario.fail_car(2).expect("fail");
    scenario.fire_recall().expect("recall");
    scenario.recover_car(2).expect("recover");
    assert_eq!(snapshot(&scenario, 2).mode, CarMode::FireRecall);
    assert!(running_until(&mut scenario, &mut now, 120_000, all_home));
}

#[test]
fn a_car_failed_during_the_recall_can_come_back_to_normal_after_it() {
    let mut scenario = started();
    scenario.fire_recall().expect("recall");
    scenario.fail_car(1).expect("fail");
    assert_eq!(snapshot(&scenario, 1).mode, CarMode::Unavailable);
    scenario.clear_fire().expect("clear");
    assert_eq!(snapshot(&scenario, 1).mode, CarMode::Unavailable);
    scenario.recover_car(1).expect("recover");
    assert_eq!(snapshot(&scenario, 1).mode, CarMode::Normal);
}

#[test]
fn a_reset_ends_the_recall() {
    let mut scenario = started();
    scenario.fire_recall().expect("recall");
    assert!(scenario.reset().is_some());
    assert!(!scenario.is_fire());
    for car in 0..CARS {
        assert_eq!(snapshot(&scenario, car).mode, CarMode::Normal);
    }
    assert!(fire_alarm(&scenario).is_none());
}

#[test]
fn a_paused_scenario_keeps_the_recall_but_does_not_move() {
    let mut scenario = started();
    let mut now = 0;
    scenario.fire_recall().expect("recall");
    scenario.set_paused(true);
    advance(&mut scenario, &mut now, 30_000);
    assert!(!all_home(&scenario));
    scenario.set_paused(false);
    assert!(running_until(&mut scenario, &mut now, 120_000, all_home));
}

#[test]
fn the_snapshot_of_a_recalled_car_has_no_assigned_calls() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(8, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 300);
    scenario.fire_recall().expect("recall");
    advance(&mut scenario, &mut now, 100);
    for car in 0..CARS {
        let s = snapshot(&scenario, car);
        assert_eq!((s.up_calls, s.down_calls), (0, 0), "car {car}");
    }
}

#[test]
fn the_recall_cancels_calls_even_when_every_car_is_failed() {
    let mut scenario = started();
    let mut now = 0;
    for car in 0..CARS {
        scenario.fail_car(car).expect("fail");
    }
    scenario.press(call(5, Direction::Up), now).expect("press");
    scenario.fire_recall().expect("recall");
    advance(&mut scenario, &mut now, 100);
    assert_eq!(count(&scenario, EventKind::Cancelled), 1);
    scenario
        .press(call(6, Direction::Down), now)
        .expect("press");
    advance(&mut scenario, &mut now, 100);
    assert_eq!(count(&scenario, EventKind::Cancelled), 2);
}
