#![allow(clippy::unwrap_used)]

use elevator_ctrl::eta::{
    Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams, eta_ms,
};
use elevator_ctrl::motion::MotionParams;
use elevator_fw_logic::car::{Car, PlantParams, Targets};
use elevator_wire::protocol::DoorState;

const HEIGHT: u32 = 3000;
const OPEN: u32 = 2000;
const DWELL: u32 = 6000;
const CLOSE: u32 = 3000;
const BRAKE: u32 = 3000;

fn motion() -> MotionParams {
    MotionParams::new(1600, 1000, 1000).unwrap()
}

fn params() -> PlantParams {
    PlantParams {
        floors: 10,
        floor_height_mm: HEIGHT,
        motion: motion(),
        open_ms: OPEN,
        dwell_ms: DWELL,
        close_ms: CLOSE,
        brake_mm_s2: BRAKE,
    }
}

fn floor_bit(floor: u32) -> u32 {
    1 << (floor - 1)
}

fn up(floors: &[u32]) -> Targets {
    Targets {
        up: floors.iter().fold(0, |mask, &f| mask | floor_bit(f)),
        down: 0,
    }
}

fn down(floors: &[u32]) -> Targets {
    Targets {
        up: 0,
        down: floors.iter().fold(0, |mask, &f| mask | floor_bit(f)),
    }
}

fn run_until(car: &mut Car, targets: Targets, limit_ms: u32, done: impl Fn(&Car) -> bool) -> u32 {
    let mut elapsed = 0;
    while !done(car) && elapsed < limit_ms {
        car.step(1, CarMode::Normal, targets);
        elapsed += 1;
    }
    elapsed
}

fn arrived(car: &Car) -> bool {
    car.stopped_at().is_some() && car.doors() != DoorState::Closed
}

#[test]
fn idle_car_stays_on_its_floor_with_closed_doors() {
    let mut car = Car::new(params(), 5);
    for _ in 0..1000 {
        car.step(10, CarMode::Normal, Targets::default());
    }
    assert_eq!(car.position_mm(), 4 * HEIGHT);
    assert_eq!(car.speed_mm_s(), 0);
    assert_eq!(car.stopped_at(), Some(5));
    assert_eq!(car.doors(), DoorState::Closed);
    assert_eq!(car.door_remaining_ms(), 0);
}

#[test]
fn a_new_car_reports_its_floor_and_heading() {
    let car = Car::new(params(), 10);
    assert_eq!(car.position_mm(), 9 * HEIGHT);
    assert_eq!(car.stopped_at(), Some(10));
    assert_eq!(car.heading(), Direction::Up);
}

#[test]
fn car_arrives_exactly_when_the_eta_said_it_would() {
    let eta_params = EtaParams {
        building: Building::new(10, HEIGHT).unwrap(),
        motion: motion(),
        doors: DoorTimes::new(OPEN, DWELL, CLOSE).unwrap(),
        aging_cap_ms: 0,
    };
    for (from, to) in [(1, 2), (1, 4), (1, 10), (3, 9), (2, 3)] {
        let state = CarState {
            position_mm: (from - 1) * HEIGHT,
            speed_mm_s: 0,
            accel_mm_s2: 0,
            heading: Direction::Up,
            door_remaining_ms: 0,
            up_calls: 0,
            down_calls: 0,
            car_calls: 0,
            mode: CarMode::Normal,
        };
        let call = Call {
            floor: u8::try_from(to).unwrap(),
            direction: Direction::Up,
        };
        let eta = eta_ms(&eta_params, &state, call).unwrap();
        let mut car = Car::new(params(), u8::try_from(from).unwrap());
        let travel = run_until(&mut car, up(&[to]), 60_000, arrived);
        assert_eq!(travel, eta, "from {from} to {to}");
        assert_eq!(car.position_mm(), (to - 1) * HEIGHT);
        assert_eq!(car.stopped_at(), Some(u8::try_from(to).unwrap()));
    }
}

#[test]
fn downward_trip_takes_the_same_time_as_the_mirrored_upward_one() {
    let mut upward = Car::new(params(), 2);
    let mut downward = Car::new(params(), 9);
    let a = run_until(&mut upward, up(&[7]), 60_000, arrived);
    let b = run_until(&mut downward, down(&[4]), 60_000, arrived);
    assert_eq!(a, b);
    assert_eq!(downward.heading(), Direction::Down);
    assert_eq!(downward.position_mm(), 3 * HEIGHT);
}

#[test]
fn while_driving_the_car_is_between_floors_with_closed_doors_and_bounded_speed() {
    let mut car = Car::new(params(), 1);
    let mut previous = car.position_mm();
    let mut peak = 0;
    let mut moving_ms = 0;
    for _ in 0..30_000 {
        car.step(1, CarMode::Normal, up(&[10]));
        if arrived(&car) {
            break;
        }
        if car.speed_mm_s() > 0 {
            moving_ms += 1;
            assert_eq!(car.doors(), DoorState::Closed);
            assert_eq!(car.stopped_at(), None);
            assert!(car.position_mm() >= previous);
            assert!(car.position_mm() - previous <= 2);
        }
        previous = car.position_mm();
        peak = peak.max(car.speed_mm_s());
    }
    assert!(moving_ms > 10_000);
    assert_eq!(peak, 1600);
    assert_eq!(car.position_mm(), 9 * HEIGHT);
}

#[test]
fn door_cycle_has_the_declared_phases_and_remaining_time() {
    let mut car = Car::new(params(), 1);
    run_until(&mut car, up(&[3]), 60_000, arrived);
    assert_eq!(car.doors(), DoorState::Opening);
    assert_eq!(car.door_remaining_ms(), OPEN + DWELL + CLOSE);
    let t = run_until(&mut car, Targets::default(), 60_000, |c| {
        c.doors() == DoorState::Open
    });
    assert_eq!(t, OPEN);
    assert_eq!(car.door_remaining_ms(), DWELL + CLOSE);
    let t = run_until(&mut car, Targets::default(), 60_000, |c| {
        c.doors() == DoorState::Closing
    });
    assert_eq!(t, DWELL);
    assert_eq!(car.door_remaining_ms(), CLOSE);
    let t = run_until(&mut car, Targets::default(), 60_000, |c| {
        c.doors() == DoorState::Closed
    });
    assert_eq!(t, CLOSE);
    assert_eq!(car.door_remaining_ms(), 0);
    assert_eq!(car.stopped_at(), Some(3));
}

#[test]
fn door_cycle_length_does_not_depend_on_the_step_size() {
    for dt in [1, 7, 10, 50, 333] {
        let mut car = Car::new(params(), 4);
        car.step(dt, CarMode::Normal, up(&[4]));
        assert_eq!(car.doors(), DoorState::Opening, "dt {dt}");
        let mut total = 0;
        while car.doors() != DoorState::Closed {
            car.step(dt, CarMode::Normal, Targets::default());
            total += dt;
        }
        assert!(total >= OPEN + DWELL + CLOSE, "dt {dt}");
        assert!(total < OPEN + DWELL + CLOSE + dt, "dt {dt}");
    }
}

#[test]
fn a_call_on_the_floor_where_the_car_stands_only_cycles_the_doors() {
    let mut car = Car::new(params(), 6);
    car.step(10, CarMode::Normal, down(&[6]));
    assert_eq!(car.doors(), DoorState::Opening);
    assert_eq!(car.position_mm(), 5 * HEIGHT);
    assert_eq!(car.speed_mm_s(), 0);
    assert_eq!(car.stopped_at(), Some(6));
}

#[test]
fn doors_never_start_a_trip_before_they_are_closed() {
    let mut car = Car::new(params(), 1);
    run_until(&mut car, up(&[2]), 60_000, arrived);
    let target = up(&[5]);
    while car.doors() != DoorState::Closed {
        assert_eq!(car.speed_mm_s(), 0);
        assert_eq!(car.position_mm(), HEIGHT);
        car.step(1, CarMode::Normal, target);
    }
    car.step(1000, CarMode::Normal, target);
    assert!(car.position_mm() > HEIGHT);
}

#[test]
fn a_target_that_appears_on_the_closing_tick_starts_the_trip_at_once() {
    let mut car = Car::new(params(), 1);
    run_until(&mut car, up(&[2]), 60_000, arrived);
    while car.doors() != DoorState::Closed {
        car.step(1, CarMode::Normal, up(&[8]));
    }
    car.step(1000, CarMode::Normal, up(&[8]));
    let mut reference = Car::new(params(), 2);
    reference.step(1, CarMode::Normal, up(&[8]));
    reference.step(1000, CarMode::Normal, up(&[8]));
    assert_eq!(car.position_mm(), reference.position_mm());
    assert_eq!(car.speed_mm_s(), reference.speed_mm_s());
}

#[test]
fn going_up_it_takes_the_nearest_up_call_and_skips_a_nearer_down_call() {
    let mut car = Car::new(params(), 1);
    let targets = Targets {
        up: floor_bit(6),
        down: floor_bit(3),
    };
    run_until(&mut car, targets, 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(6));
    assert_eq!(car.heading(), Direction::Up);
}

#[test]
fn going_up_it_turns_at_the_highest_call_when_no_up_call_is_ahead() {
    let mut car = Car::new(params(), 1);
    let targets = Targets {
        up: 0,
        down: floor_bit(4) | floor_bit(8),
    };
    run_until(&mut car, targets, 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(8));
    assert_eq!(car.heading(), Direction::Up);
}

#[test]
fn with_nothing_ahead_it_reverses_and_serves_the_nearest_down_call() {
    let mut car = Car::new(params(), 9);
    let targets = Targets {
        up: 0,
        down: floor_bit(7) | floor_bit(2),
    };
    run_until(&mut car, targets, 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(7));
    assert_eq!(car.heading(), Direction::Down);
}

#[test]
fn going_down_it_turns_at_the_lowest_call_when_no_down_call_is_ahead() {
    let mut car = Car::new(params(), 9);
    let targets = Targets {
        up: floor_bit(2) | floor_bit(5),
        down: 0,
    };
    run_until(&mut car, targets, 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(2));
    assert_eq!(car.heading(), Direction::Down);
}

#[test]
fn a_car_that_cannot_go_further_up_turns_down_for_calls_below() {
    let mut car = Car::new(params(), 10);
    let targets = up(&[3]);
    run_until(&mut car, targets, 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(3));
    assert_eq!(car.heading(), Direction::Down);
}

#[test]
fn an_unavailable_car_never_leaves_its_floor() {
    let mut car = Car::new(params(), 3);
    for _ in 0..5000 {
        car.step(10, CarMode::Unavailable, up(&[9]));
    }
    assert_eq!(car.position_mm(), 2 * HEIGHT);
    assert_eq!(car.doors(), DoorState::Closed);
}

#[test]
fn a_failure_during_a_trip_brakes_without_a_jump_and_stays_put() {
    let mut car = Car::new(params(), 1);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, up(&[10]));
    }
    let speed_at_failure = car.speed_mm_s();
    assert!(speed_at_failure > 1000);
    let mut previous = car.position_mm();
    let mut previous_speed = speed_at_failure;
    let mut braking_ms = 0;
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, up(&[10]));
        assert!(car.position_mm() >= previous);
        assert!(car.position_mm() - previous <= 2);
        assert!(car.speed_mm_s() <= previous_speed);
        previous = car.position_mm();
        previous_speed = car.speed_mm_s();
        braking_ms += 1;
        assert!(braking_ms < 2000);
    }
    assert_eq!(braking_ms, (1000 * speed_at_failure).div_ceil(BRAKE));
    let stopped = car.position_mm();
    assert_eq!(car.stopped_at(), None);
    assert_eq!(car.doors(), DoorState::Closed);
    for _ in 0..5000 {
        car.step(10, CarMode::Unavailable, up(&[10]));
    }
    assert_eq!(car.position_mm(), stopped);
    assert_eq!(car.speed_mm_s(), 0);
}

#[test]
fn the_braking_distance_matches_constant_deceleration() {
    let mut car = Car::new(params(), 1);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, up(&[10]));
    }
    let (start, speed) = (car.position_mm(), car.speed_mm_s());
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    let expected = u64::from(speed) * u64::from(speed) / (2 * u64::from(BRAKE));
    let covered = u64::from(car.position_mm() - start);
    assert!(covered.abs_diff(expected) <= 2, "{covered} vs {expected}");
}

#[test]
fn a_recovered_car_resumes_from_between_floors() {
    let mut car = Car::new(params(), 1);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, up(&[10]));
    }
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    assert_eq!(car.stopped_at(), None);
    run_until(&mut car, up(&[6]), 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(6));
    assert_eq!(car.position_mm(), 5 * HEIGHT);
}

#[test]
fn a_recovered_car_below_its_target_goes_back_down_to_it() {
    let mut car = Car::new(params(), 1);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, up(&[10]));
    }
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    run_until(&mut car, up(&[1]), 60_000, arrived);
    assert_eq!(car.stopped_at(), Some(1));
    assert_eq!(car.heading(), Direction::Down);
}

#[test]
fn braking_never_pushes_the_car_past_the_end_of_the_shaft() {
    let mut car = Car::new(params(), 9);
    let top = 9 * HEIGHT;
    for _ in 0..20_000 {
        car.step(1, CarMode::Normal, up(&[10]));
        if car.position_mm() > top - 200 && car.speed_mm_s() > 0 {
            break;
        }
    }
    assert!(car.speed_mm_s() > 0);
    for _ in 0..2000 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    assert!(car.position_mm() <= top);
}

#[test]
fn a_failure_while_the_doors_are_open_lets_them_finish_the_cycle() {
    let mut car = Car::new(params(), 2);
    car.step(1, CarMode::Normal, up(&[2]));
    assert_eq!(car.doors(), DoorState::Opening);
    for _ in 0..OPEN + DWELL + CLOSE {
        car.step(1, CarMode::Unavailable, up(&[2]));
    }
    assert_eq!(car.doors(), DoorState::Closed);
    car.step(1, CarMode::Unavailable, up(&[2]));
    assert_eq!(car.doors(), DoorState::Closed);
}

#[test]
fn a_zero_step_changes_nothing() {
    let mut car = Car::new(params(), 1);
    car.step(0, CarMode::Normal, up(&[5]));
    car.step(0, CarMode::Normal, up(&[5]));
    assert_eq!(car.position_mm(), 0);
    assert_eq!(car.doors(), DoorState::Closed);
}

#[test]
fn a_single_huge_step_ends_the_trip_without_overshoot() {
    let mut car = Car::new(params(), 1);
    car.step(1, CarMode::Normal, up(&[10]));
    car.step(u32::MAX, CarMode::Normal, up(&[10]));
    assert_eq!(car.position_mm(), 9 * HEIGHT);
    assert_eq!(car.speed_mm_s(), 0);
    assert_eq!(car.stopped_at(), Some(10));
}

#[test]
fn remaining_door_time_counts_down_inside_every_phase() {
    let mut car = Car::new(params(), 4);
    car.step(1, CarMode::Normal, up(&[4]));
    car.step(500, CarMode::Normal, Targets::default());
    assert_eq!(car.doors(), DoorState::Opening);
    assert_eq!(car.door_remaining_ms(), OPEN + DWELL + CLOSE - 500);
    car.step(OPEN, CarMode::Normal, Targets::default());
    assert_eq!(car.doors(), DoorState::Open);
    assert_eq!(car.door_remaining_ms(), DWELL + CLOSE - 500);
    car.step(DWELL - 500 + 700, CarMode::Normal, Targets::default());
    assert_eq!(car.doors(), DoorState::Closing);
    assert_eq!(car.door_remaining_ms(), CLOSE - 700);
}

#[test]
fn a_downward_trip_brakes_downward() {
    let mut car = Car::new(params(), 10);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, down(&[1]));
    }
    let before = car.position_mm();
    assert!(before < 9 * HEIGHT);
    let speed = car.speed_mm_s();
    assert!(speed > 0);
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, down(&[1]));
        assert!(car.position_mm() <= before);
    }
    assert!(car.position_mm() < before);
    assert_eq!(car.heading(), Direction::Down);
}

#[test]
fn a_weak_brake_cannot_carry_the_car_through_the_roof() {
    let weak = PlantParams {
        brake_mm_s2: 100,
        ..params()
    };
    let mut car = Car::new(weak, 9);
    let top = 9 * HEIGHT;
    for _ in 0..20_000 {
        car.step(1, CarMode::Normal, up(&[10]));
        if car.position_mm() > top - 300 {
            break;
        }
    }
    assert!(car.speed_mm_s() > 0);
    for _ in 0..30_000 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    assert_eq!(car.speed_mm_s(), 0);
    assert_eq!(car.position_mm(), top);
}

#[test]
fn braking_is_over_on_the_step_that_reaches_zero_speed() {
    let mut car = Car::new(params(), 1);
    for _ in 0..4000 {
        car.step(1, CarMode::Normal, up(&[10]));
    }
    while car.speed_mm_s() > 0 {
        car.step(1, CarMode::Unavailable, up(&[10]));
    }
    let stopped = car.position_mm();
    car.step(1000, CarMode::Normal, up(&[10]));
    assert!(car.position_mm() > stopped);
}

#[test]
fn the_current_heading_is_served_before_the_opposite_one() {
    let mut car = Car::new(params(), 5);
    let both = Targets {
        up: floor_bit(8),
        down: floor_bit(2),
    };
    run_until(&mut car, both, 120_000, arrived);
    assert_eq!(car.stopped_at(), Some(8));
    let mut car = Car::new(params(), 10);
    run_until(&mut car, down(&[6]), 120_000, arrived);
    while car.doors() != DoorState::Closed {
        car.step(1, CarMode::Normal, Targets::default());
    }
    assert_eq!(car.heading(), Direction::Down);
    let both = Targets {
        up: floor_bit(9),
        down: floor_bit(2),
    };
    run_until(&mut car, both, 120_000, arrived);
    assert_eq!(car.stopped_at(), Some(2));
}

#[test]
fn a_zero_floor_height_does_not_crash_or_report_a_floor() {
    let degenerate = PlantParams {
        floor_height_mm: 0,
        ..params()
    };
    let mut car = Car::new(degenerate, 1);
    car.step(10, CarMode::Normal, up(&[3]));
    assert_eq!(car.stopped_at(), None);
}

fn eta_params() -> EtaParams {
    EtaParams {
        building: Building::new(10, HEIGHT).unwrap(),
        motion: motion(),
        doors: DoorTimes::new(OPEN, DWELL, CLOSE).unwrap(),
        aging_cap_ms: 0,
    }
}

fn observed(car: &Car, targets: Targets) -> CarState {
    CarState {
        position_mm: car.position_mm(),
        speed_mm_s: car.speed_mm_s(),
        accel_mm_s2: u32::try_from(car.accel_mm_s2().max(0)).unwrap(),
        heading: car.heading(),
        door_remaining_ms: car.door_remaining_ms(),
        up_calls: targets.up,
        down_calls: targets.down,
        car_calls: 0,
        mode: CarMode::Normal,
    }
}

fn drive(car: &mut Car, targets: Targets, millis: u32) {
    for _ in 0..millis {
        car.step(1, CarMode::Normal, targets);
    }
}

fn stops(car: &mut Car, targets: Targets, count: usize) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut targets = targets;
    while seen.len() < count {
        let waited = run_until(car, targets, 120_000, arrived);
        assert!(waited < 120_000, "stuck after {seen:?}");
        let floor = car.stopped_at().unwrap();
        seen.push(floor);
        let bit = !floor_bit(u32::from(floor));
        targets = Targets {
            up: targets.up & bit,
            down: targets.down & bit,
        };
        run_until(car, targets, 60_000, |car| car.doors() == DoorState::Closed);
    }
    seen
}

#[test]
fn an_up_call_ahead_stops_a_moving_car_exactly_when_the_eta_said() {
    for (from, departed_ms, call, also) in [
        (1, 1500, 5, 10),
        (1, 600, 2, 10),
        (1, 4000, 6, 10),
        (3, 2500, 7, 10),
        (1, 1500, 5, 7),
    ] {
        let mut car = Car::new(params(), from);
        drive(&mut car, up(&[10]), departed_ms);
        assert!(car.speed_mm_s() > 0);
        let both = up(&[call, also, 10]);
        let eta = eta_ms(
            &eta_params(),
            &observed(&car, up(&[also, 10])),
            Call {
                floor: u8::try_from(call).unwrap(),
                direction: Direction::Up,
            },
        )
        .unwrap();
        let travel = run_until(&mut car, both, 60_000, arrived);
        let case = format!("from {from} after {departed_ms} ms to {call}");
        assert_eq!(
            car.stopped_at(),
            Some(u8::try_from(call).unwrap()),
            "{case}"
        );
        assert_eq!(travel, eta, "{case}");
        assert_eq!(car.position_mm(), (call - 1) * HEIGHT, "{case}");
    }
}

#[test]
fn a_down_call_ahead_stops_a_car_going_down() {
    let mut car = Car::new(params(), 10);
    drive(&mut car, down(&[1]), 2000);
    assert_eq!(car.heading(), Direction::Down);
    assert_eq!(stops(&mut car, down(&[4, 1]), 2), [4, 1]);
}

#[test]
fn a_call_too_close_to_stop_for_is_served_after_the_far_stop() {
    let mut car = Car::new(params(), 1);
    let mut elapsed = 0;
    while {
        let stop = motion()
            .stopping_distance_mm(
                car.speed_mm_s(),
                u32::try_from(car.accel_mm_s2().max(0)).unwrap(),
            )
            .unwrap();
        car.position_mm() + stop <= 7 * HEIGHT
    } {
        car.step(1, CarMode::Normal, up(&[10]));
        elapsed += 1;
        assert!(elapsed < 60_000);
    }
    assert!(car.position_mm() < 7 * HEIGHT);
    assert_eq!(stops(&mut car, up(&[8, 9, 10]), 3), [9, 10, 8]);
}

#[test]
fn a_call_in_the_opposite_direction_does_not_stop_a_moving_car() {
    let mut car = Car::new(params(), 1);
    drive(&mut car, up(&[10]), 1500);
    let targets = Targets {
        up: floor_bit(10),
        down: floor_bit(6),
    };
    assert_eq!(stops(&mut car, targets, 2), [10, 6]);
}

#[test]
fn several_calls_on_the_way_are_served_in_floor_order() {
    let mut car = Car::new(params(), 1);
    drive(&mut car, up(&[10]), 800);
    drive(&mut car, up(&[8, 10]), 700);
    assert_eq!(stops(&mut car, up(&[4, 8, 10]), 3), [4, 8, 10]);
}

#[test]
fn changing_the_stop_on_the_way_moves_the_car_without_jumps() {
    for departed_ms in [300, 1000, 1500, 2600, 3500] {
        let mut car = Car::new(params(), 1);
        drive(&mut car, up(&[10]), departed_ms);
        let mut previous = (car.position_mm(), car.speed_mm_s(), car.accel_mm_s2());
        let mut elapsed = 0;
        while !arrived(&car) {
            car.step(1, CarMode::Normal, up(&[6, 10]));
            let now = (car.position_mm(), car.speed_mm_s(), car.accel_mm_s2());
            let case = format!("departed {departed_ms} t={elapsed} {previous:?} -> {now:?}");
            assert!(now.0 >= previous.0 && now.0 - previous.0 <= 2, "{case}");
            assert!(now.1.abs_diff(previous.1) <= 2, "{case}");
            assert!(now.2.abs_diff(previous.2) <= 50, "{case}");
            assert!(now.1 <= 1600 && now.2.abs() <= 1010, "{case}");
            previous = now;
            elapsed += 1;
            assert!(elapsed < 60_000);
        }
        assert_eq!(car.stopped_at(), Some(6), "departed {departed_ms}");
    }
}

#[test]
fn a_car_at_rest_reports_no_acceleration_and_a_departing_one_speeds_up() {
    let mut car = Car::new(params(), 1);
    assert_eq!(car.accel_mm_s2(), 0);
    drive(&mut car, up(&[10]), 1500);
    assert!(car.accel_mm_s2() > 900);
    run_until(&mut car, up(&[10]), 60_000, arrived);
    assert_eq!(car.accel_mm_s2(), 0);
}

#[test]
fn with_low_floors_it_stops_at_the_first_floor_it_still_can() {
    let low = PlantParams {
        floor_height_mm: 1000,
        ..params()
    };
    let mut car = Car::new(low, 1);
    let mut elapsed = 0;
    while {
        let stop = motion()
            .stopping_distance_mm(
                car.speed_mm_s(),
                u32::try_from(car.accel_mm_s2().max(0)).unwrap(),
            )
            .unwrap();
        car.position_mm() + stop <= 5000
    } {
        car.step(1, CarMode::Normal, up(&[10]));
        elapsed += 1;
        assert!(elapsed < 60_000);
    }
    assert_eq!(car.speed_mm_s(), 1600);
    assert_eq!(stops(&mut car, up(&[6, 8, 10]), 3), [8, 10, 6]);
}

#[test]
fn emergency_braking_reports_the_brake_deceleration_until_the_car_stops() {
    let mut car = Car::new(params(), 1);
    drive(&mut car, up(&[10]), 4000);
    car.step(10, CarMode::Unavailable, up(&[10]));
    assert!(car.speed_mm_s() > 0);
    assert_eq!(car.accel_mm_s2(), -i64::from(BRAKE));
    for _ in 0..1000 {
        car.step(10, CarMode::Unavailable, up(&[10]));
    }
    assert_eq!(car.speed_mm_s(), 0);
    assert_eq!(car.accel_mm_s2(), 0);
}

#[test]
fn going_down_with_low_floors_it_stops_at_the_first_floor_it_still_can() {
    let low = PlantParams {
        floor_height_mm: 1000,
        ..params()
    };
    let mut car = Car::new(low, 10);
    let mut elapsed = 0;
    while {
        let stop = motion()
            .stopping_distance_mm(
                car.speed_mm_s(),
                u32::try_from(car.accel_mm_s2().max(0)).unwrap(),
            )
            .unwrap();
        car.position_mm() >= 4000 + stop
    } {
        car.step(1, CarMode::Normal, down(&[1]));
        elapsed += 1;
        assert!(elapsed < 60_000);
    }
    assert_eq!(car.speed_mm_s(), 1600);
    assert_eq!(stops(&mut car, down(&[5, 3, 1]), 3), [3, 1, 5]);
}
