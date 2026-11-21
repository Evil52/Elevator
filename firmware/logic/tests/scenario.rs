use elevator_fw_logic::scenario::{CALLS, Scenario};

#[test]
fn restart_registers_every_call_slot() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.restart(0), Some(CALLS));
}

#[test]
fn first_step_gives_every_idle_car_one_call() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert_eq!(scenario.step(10), 3);
}

#[test]
fn all_calls_are_assigned_within_six_steps_and_then_it_is_quiet() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    let assigned: usize = (1..=6).map(|tick| scenario.step(tick * 10)).sum();
    assert_eq!(assigned, CALLS);
    assert_eq!(scenario.step(70), 0);
}

#[test]
fn restart_starts_over_with_a_fresh_queue() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    for tick in 1..=7 {
        scenario.step(tick * 10);
    }
    assert_eq!(scenario.restart(1000), Some(CALLS));
    assert_eq!(scenario.step(1010), 3);
}

#[test]
fn stepping_before_a_restart_does_nothing() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.step(10), 0);
}
