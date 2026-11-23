#![allow(clippy::expect_used)]

use elevator_ctrl::eta::{Call, Direction};
use elevator_fw_logic::scenario::{CALLS, CARS, Scenario};
use elevator_wire::protocol::{DoorState, Event, EventKind};

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

fn moving_car(scenario: &Scenario) -> Option<usize> {
    (0..CARS).find(|&car| {
        scenario
            .car_snapshot(car, 0)
            .is_some_and(|snapshot| snapshot.speed_mm_per_sec != 0)
    })
}

fn started() -> Scenario {
    let mut scenario = Scenario::new();
    assert!(scenario.reset().is_some());
    scenario
}

fn positions(scenario: &Scenario) -> [u32; CARS] {
    core::array::from_fn(|car| scenario.car_snapshot(car, 0).expect("car").position_mm)
}

#[test]
fn idle_cars_stay_where_they_are() {
    let mut scenario = started();
    let before = positions(&scenario);
    let mut now = 0;
    advance(&mut scenario, &mut now, 60_000);
    assert_eq!(positions(&scenario), before);
}

#[test]
fn a_pressed_call_is_driven_to_and_served_with_open_doors() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(4, Direction::Up), now).expect("press");
    assert!(running_until(&mut scenario, &mut now, 60_000, |s| {
        count(s, EventKind::Served) == 1
    }));
    let served = events_of(&scenario)
        .into_iter()
        .find(|event| event.kind == EventKind::Served)
        .expect("served");
    let car = usize::from(served.car.expect("car"));
    let snapshot = scenario.car_snapshot(car, now).expect("snapshot");
    assert_eq!(snapshot.floor, Some(4));
    assert_eq!(snapshot.doors, DoorState::Open);
    assert_eq!(snapshot.speed_mm_per_sec, 0);
    assert_eq!(count(&scenario, EventKind::Arrived), 1);
}

#[test]
fn the_call_is_served_no_earlier_than_the_trip_and_door_opening_allow() {
    let mut scenario = started();
    let mut now = 0;
    scenario
        .press(call(10, Direction::Down), now)
        .expect("press");
    assert!(running_until(&mut scenario, &mut now, 1_000, |s| {
        count(s, EventKind::Assigned) == 1
    }));
    advance(&mut scenario, &mut now, 1_000);
    assert_eq!(count(&scenario, EventKind::Served), 0);
}

#[test]
fn a_car_standing_on_the_floor_of_the_call_only_opens_its_doors() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(1, Direction::Up), now).expect("press");
    assert!(running_until(&mut scenario, &mut now, 10_000, |s| {
        count(s, EventKind::Served) == 1
    }));
    assert!(now >= 2_000);
    assert!(now < 2_200);
    assert_eq!(positions(&scenario)[0], 0);
}

#[test]
fn snapshots_show_a_car_between_floors_with_speed_and_closed_doors() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(8, Direction::Up), now).expect("press");
    let mut seen_moving = false;
    for _ in 0..3_000 {
        now += STEP_MS;
        scenario.step(now);
        for car in 0..CARS {
            let snapshot = scenario.car_snapshot(car, now).expect("snapshot");
            if snapshot.speed_mm_per_sec != 0 {
                seen_moving = true;
                assert_eq!(snapshot.floor, None);
                assert_eq!(snapshot.doors, DoorState::Closed);
            }
        }
    }
    assert!(seen_moving);
}

#[test]
fn snapshots_report_all_door_phases_in_order() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(1, Direction::Up), now).expect("press");
    let mut phases = Vec::new();
    for _ in 0..1_500 {
        now += STEP_MS;
        scenario.step(now);
        let doors = scenario.car_snapshot(0, now).expect("snapshot").doors;
        if phases.last() != Some(&doors) {
            phases.push(doors);
        }
    }
    assert_eq!(
        phases,
        [
            DoorState::Closed,
            DoorState::Opening,
            DoorState::Open,
            DoorState::Closing,
            DoorState::Closed
        ]
    );
}

#[test]
fn a_paused_scenario_freezes_the_cars_and_resuming_does_not_make_them_jump() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(9, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 3_000);
    let before = positions(&scenario);
    assert!(before != [0, 12_000, 27_000]);
    scenario.set_paused(true);
    advance(&mut scenario, &mut now, 20_000);
    assert_eq!(positions(&scenario), before);
    scenario.set_paused(false);
    advance(&mut scenario, &mut now, STEP_MS);
    let after = positions(&scenario);
    for car in 0..CARS {
        assert!(after[car].abs_diff(before[car]) <= 20, "car {car}");
    }
}

#[test]
fn a_car_failed_on_the_way_brakes_and_another_car_serves_the_call() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(4, Direction::Up), now).expect("press");
    assert!(running_until(&mut scenario, &mut now, 20_000, |s| {
        moving_car(s).is_some()
    }));
    let mover = moving_car(&scenario).expect("moving car");
    scenario.fail_car(mover).expect("fail");
    advance(&mut scenario, &mut now, 5_000);
    let stopped = scenario.car_snapshot(mover, now).expect("snapshot");
    assert_eq!(stopped.speed_mm_per_sec, 0);
    assert_eq!(stopped.mode, elevator_wire::protocol::CarMode::Unavailable);
    assert!(running_until(&mut scenario, &mut now, 120_000, |s| {
        count(s, EventKind::Served) == 1
    }));
    let served = events_of(&scenario)
        .into_iter()
        .find(|event| event.kind == EventKind::Served)
        .expect("served");
    assert_ne!(served.car, Some(u8::try_from(mover).expect("car id")));
    assert_eq!(
        scenario
            .car_snapshot(mover, now)
            .expect("snapshot")
            .position_mm,
        stopped.position_mm
    );
    assert!(count(&scenario, EventKind::Requeued) >= 1);
}

#[test]
fn a_recovered_car_takes_calls_again_from_where_it_stopped() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(4, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 3_000);
    let mover = moving_car(&scenario).expect("moving car");
    scenario.fail_car(mover).expect("fail");
    advance(&mut scenario, &mut now, 5_000);
    scenario.recover_car(mover).expect("recover");
    scenario
        .press(call(7, Direction::Down), now)
        .expect("press");
    assert!(running_until(&mut scenario, &mut now, 180_000, |s| {
        count(s, EventKind::Served) == 2
    }));
}

#[test]
fn reset_puts_the_cars_back_on_their_start_floors_with_closed_doors() {
    let mut scenario = started();
    let mut now = 0;
    scenario
        .press(call(9, Direction::Down), now)
        .expect("press");
    advance(&mut scenario, &mut now, 8_000);
    assert!(scenario.reset().is_some());
    assert_eq!(positions(&scenario), [0, 12_000, 27_000]);
    for car in 0..CARS {
        let snapshot = scenario.car_snapshot(car, now).expect("snapshot");
        assert_eq!(snapshot.doors, DoorState::Closed);
        assert_eq!(snapshot.speed_mm_per_sec, 0);
    }
}

#[test]
fn restart_with_every_call_pressed_serves_all_of_them() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    let mut now = 0;
    assert!(running_until(&mut scenario, &mut now, 900_000, |s| {
        count(s, EventKind::Served) == CALLS
    }));
    assert_eq!(count(&scenario, EventKind::Requeued), 0);
}

#[test]
fn a_snapshot_lists_the_calls_assigned_to_that_car_until_they_are_served() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(7, Direction::Up), now).expect("press");
    scenario
        .press(call(3, Direction::Down), now)
        .expect("press");
    advance(&mut scenario, &mut now, 100);
    let mut up = 0;
    let mut down = 0;
    for car in 0..CARS {
        let snapshot = scenario.car_snapshot(car, now).expect("snapshot");
        up |= snapshot.up_calls;
        down |= snapshot.down_calls;
    }
    assert_eq!(up, 1 << 6);
    assert_eq!(down, 1 << 2);
    assert!(running_until(&mut scenario, &mut now, 180_000, |s| {
        count(s, EventKind::Served) == 2
    }));
    advance(&mut scenario, &mut now, 100);
    for car in 0..CARS {
        let snapshot = scenario.car_snapshot(car, now).expect("snapshot");
        assert_eq!((snapshot.up_calls, snapshot.down_calls), (0, 0));
    }
}

#[test]
fn a_car_driving_down_reports_a_negative_speed() {
    let mut scenario = started();
    let mut now = 0;
    scenario
        .press(call(8, Direction::Down), now)
        .expect("press");
    let mut seen = Vec::new();
    for _ in 0..1_500 {
        now += STEP_MS;
        scenario.step(now);
        for car in 0..CARS {
            let speed = scenario
                .car_snapshot(car, now)
                .expect("snapshot")
                .speed_mm_per_sec;
            if speed != 0 {
                seen.push(speed);
            }
        }
    }
    assert!(seen.len() > 10);
    assert!(seen.iter().all(|speed| *speed < 0));
}

#[test]
fn the_dispatcher_counts_the_door_cycle_of_a_car_that_is_busy_at_its_floor() {
    let mut scenario = started();
    let mut now = 0;
    scenario.press(call(1, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 1_000);
    scenario.press(call(2, Direction::Up), now).expect("press");
    advance(&mut scenario, &mut now, 100);
    let assigned: Vec<Event> = events_of(&scenario)
        .into_iter()
        .filter(|event| event.kind == EventKind::Assigned)
        .collect();
    assert_eq!(assigned.len(), 2);
    assert_eq!(assigned[0].car, Some(0));
    assert_eq!(assigned[1].car, Some(1));
}
