use elevator_fw_logic::scenario::{CALLS, CARS, Scenario, StepEvents};
use elevator_wire::protocol::{CarMode, DoorState};

#[test]
fn restart_registers_every_call_slot() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.restart(0), Some(CALLS));
}

#[test]
fn first_step_gives_every_idle_car_one_call() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert_eq!(scenario.step(10).assigned, 3);
}

#[test]
fn all_calls_are_assigned_within_six_steps_and_then_it_is_quiet() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    let assigned: usize = (1..=6).map(|tick| scenario.step(tick * 10).assigned).sum();
    assert_eq!(assigned, CALLS);
    assert_eq!(scenario.step(70).assigned, 0);
}

#[test]
fn restart_starts_over_with_a_fresh_queue() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    for tick in 1..=7 {
        scenario.step(tick * 10);
    }
    assert_eq!(scenario.restart(1000), Some(CALLS));
    assert_eq!(scenario.step(1010).assigned, 3);
}

#[test]
fn stepping_before_a_restart_does_nothing() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.step(10), StepEvents::default());
}

#[test]
fn a_failed_car_gets_its_calls_requeued() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert_eq!(scenario.step(10).assigned, 3);
    assert!(scenario.toggle_failure(0));
    assert!(scenario.is_failed(0));
    assert_eq!(scenario.step(20).requeued, 1);
}

#[test]
fn a_failed_car_gets_no_new_calls() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert!(scenario.toggle_failure(0));
    assert_eq!(scenario.step(10).assigned, CARS - 1);
}

#[test]
fn toggling_twice_brings_the_car_back() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert!(scenario.toggle_failure(1));
    assert!(!scenario.toggle_failure(1));
    assert!(!scenario.is_failed(1));
    assert_eq!(scenario.step(10).assigned, CARS);
}

#[test]
fn an_unknown_car_cannot_be_failed() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert!(!scenario.toggle_failure(CARS));
    assert!(!scenario.is_failed(CARS));
    assert_eq!(scenario.step(10).assigned, CARS);
}

#[test]
fn restart_clears_failures() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.toggle_failure(2);
    scenario.restart(1000);
    assert!(!scenario.is_failed(2));
    assert_eq!(scenario.step(1010).assigned, CARS);
}

#[test]
fn reload_refills_the_queue_but_keeps_failures() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.toggle_failure(2);
    for tick in 1..=7 {
        scenario.step(tick * 10);
    }
    assert_eq!(scenario.reload(1000), Some(CALLS));
    assert!(scenario.is_failed(2));
    assert_eq!(scenario.step(1010).assigned, CARS - 1);
}

#[test]
fn a_snapshot_describes_an_idle_car_on_its_floor() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    let snapshot = scenario.car_snapshot(1, 4_321).expect("snapshot");
    assert_eq!(snapshot.car, 1);
    assert_eq!(snapshot.mode, CarMode::Normal);
    assert_eq!(snapshot.doors, DoorState::Closed);
    assert_eq!(snapshot.floor, Some(5));
    assert_eq!(snapshot.position_mm, 12_000);
    assert_eq!(snapshot.speed_mm_per_sec, 0);
    assert_eq!(snapshot.eta_ms, None);
    assert_eq!(snapshot.model_time_ms, 4_321);
}

#[test]
fn snapshots_cover_the_first_and_the_top_floor() {
    let scenario = Scenario::new();
    assert_eq!(scenario.car_snapshot(0, 0).expect("first").floor, Some(1));
    assert_eq!(scenario.car_snapshot(2, 0).expect("top").floor, Some(10));
}

#[test]
fn a_failed_car_reports_unavailable() {
    let mut scenario = Scenario::new();
    scenario.toggle_failure(0);
    assert_eq!(
        scenario.car_snapshot(0, 0).expect("snapshot").mode,
        CarMode::Unavailable
    );
    assert_eq!(
        scenario.car_snapshot(1, 0).expect("snapshot").mode,
        CarMode::Normal
    );
}

#[test]
fn an_unknown_car_has_no_snapshot() {
    assert_eq!(Scenario::new().car_snapshot(CARS, 0), None);
}
