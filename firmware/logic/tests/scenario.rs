use elevator_ctrl::calls::{Press, Rejection};
use elevator_ctrl::eta::{Call, Direction};
use elevator_fw_logic::alarms::AlarmError;
use elevator_fw_logic::scenario::{CALLS, CARS, CarError, PressError, Scenario, StepEvents};
use elevator_wire::protocol::{AlarmKind, AlarmState, CarMode, DoorState, Event, EventKind};

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

fn call(floor: u8, direction: Direction) -> Call {
    Call { floor, direction }
}

#[test]
fn pressing_before_the_scenario_runs_is_refused() {
    let mut scenario = Scenario::new();
    assert_eq!(
        scenario.press(call(3, Direction::Down), 0),
        Err(PressError::NotRunning)
    );
}

#[test]
fn an_empty_start_has_no_calls_until_one_is_pressed() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.start_empty(), Some(()));
    assert_eq!(scenario.step(10), StepEvents::default());
    assert!(matches!(
        scenario.press(call(3, Direction::Down), 20),
        Ok(Press::Created(_))
    ));
    assert_eq!(scenario.step(30).assigned, 1);
}

#[test]
fn pressing_the_same_call_twice_makes_one_call() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    let first = scenario.press(call(8, Direction::Up), 0);
    let second = scenario.press(call(8, Direction::Up), 5);
    let (Ok(Press::Created(id)), Ok(Press::Duplicate(again))) = (first, second) else {
        panic!("expected a created call and a duplicate: {first:?} {second:?}");
    };
    assert_eq!(id, again);
    assert_eq!(scenario.step(10).assigned, 1);
}

#[test]
fn impossible_buttons_are_rejected_with_the_reason() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    assert_eq!(
        scenario.press(call(0, Direction::Up), 0),
        Err(PressError::Call(Rejection::NoSuchFloor))
    );
    assert_eq!(
        scenario.press(call(11, Direction::Down), 0),
        Err(PressError::Call(Rejection::NoSuchFloor))
    );
    assert_eq!(
        scenario.press(call(1, Direction::Down), 0),
        Err(PressError::Call(Rejection::DownFromFirst))
    );
    assert_eq!(
        scenario.press(call(10, Direction::Up), 0),
        Err(PressError::Call(Rejection::UpFromTop))
    );
    assert_eq!(scenario.step(10), StepEvents::default());
}

#[test]
fn a_car_can_be_failed_once() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.fail_car(0), Ok(()));
    assert!(scenario.is_failed(0));
    assert_eq!(scenario.fail_car(0), Err(CarError::WrongState));
    assert_eq!(scenario.fail_car(CARS), Err(CarError::NoSuchCar));
    assert!(!scenario.is_failed(1));
}

#[test]
fn only_a_failed_car_can_be_recovered() {
    let mut scenario = Scenario::new();
    assert_eq!(scenario.recover_car(1), Err(CarError::WrongState));
    scenario.fail_car(1).ok();
    assert_eq!(scenario.recover_car(1), Ok(()));
    assert!(!scenario.is_failed(1));
    assert_eq!(scenario.recover_car(CARS), Err(CarError::NoSuchCar));
}

#[test]
fn a_paused_scenario_does_nothing_until_resumed() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    assert!(scenario.set_paused(true));
    assert!(scenario.is_paused());
    assert!(!scenario.set_paused(true));
    assert_eq!(scenario.step(10), StepEvents::default());
    assert!(scenario.set_paused(false));
    assert!(!scenario.is_paused());
    assert!(!scenario.set_paused(false));
    assert_eq!(scenario.step(20).assigned, 3);
}

#[test]
fn restart_resumes_a_paused_scenario() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.set_paused(true);
    scenario.restart(100);
    assert!(!scenario.is_paused());
}

#[test]
fn reset_gives_an_empty_healthy_running_scenario() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.fail_car(0).ok();
    scenario.set_paused(true);
    assert_eq!(scenario.reset(), Some(()));
    assert!(!scenario.is_failed(0));
    assert!(!scenario.is_paused());
    assert_eq!(scenario.step(10), StepEvents::default());
    assert!(matches!(
        scenario.press(call(3, Direction::Down), 20),
        Ok(Press::Created(_))
    ));
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

fn kinds(events: &[Event]) -> Vec<EventKind> {
    events.iter().map(|event| event.kind).collect()
}

#[test]
fn assignments_are_logged_with_car_call_and_time() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.step(10);
    let events = events_of(&scenario);
    assert_eq!(kinds(&events), vec![EventKind::Assigned; 3]);
    assert!(events.iter().all(|event| event.at_ms == 10));
    let mut cars: Vec<Option<u8>> = events.iter().map(|event| event.car).collect();
    cars.sort();
    assert_eq!(cars, vec![Some(0), Some(1), Some(2)]);
    assert_eq!(
        events.iter().map(|event| event.index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn a_failed_car_raises_an_alarm_and_a_recovery_clears_it() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    let before = scenario.alarm_version();
    scenario.step(40);
    scenario.fail_car(1).ok();
    let page = scenario.alarm_page(0);
    assert_eq!(page.total, 1);
    let alarm = page.items[0].expect("alarm");
    assert_eq!(
        (alarm.kind, alarm.car, alarm.state),
        (AlarmKind::CarFault, Some(1), AlarmState::Active)
    );
    assert_eq!(alarm.raised_at_ms, 40);
    assert_ne!(scenario.alarm_version(), before);
    let raised = events_of(&scenario);
    assert_eq!(kinds(&raised), vec![EventKind::AlarmRaised]);
    assert_eq!((raised[0].car, raised[0].id), (Some(1), alarm.id));
    scenario.step(50);
    scenario.recover_car(1).ok();
    assert_eq!(
        scenario.alarm_page(0).items[0].map(|a| a.state),
        Some(AlarmState::Cleared)
    );
    assert_eq!(
        kinds(&events_of(&scenario)),
        vec![EventKind::AlarmRaised, EventKind::AlarmCleared]
    );
}

#[test]
fn the_button_toggle_raises_and_clears_the_same_alarm() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    scenario.toggle_failure(0);
    assert_eq!(
        scenario.alarm_page(0).items[0].map(|a| a.state),
        Some(AlarmState::Active)
    );
    scenario.toggle_failure(0);
    assert_eq!(
        scenario.alarm_page(0).items[0].map(|a| a.state),
        Some(AlarmState::Cleared)
    );
}

#[test]
fn failing_a_failed_car_again_makes_no_second_alarm() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    scenario.fail_car(0).ok();
    assert_eq!(scenario.fail_car(0), Err(CarError::WrongState));
    assert_eq!(scenario.alarm_page(0).total, 1);
}

#[test]
fn requeued_calls_of_a_failed_car_are_logged() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.step(10);
    scenario.fail_car(0).ok();
    scenario.step(20);
    let events = events_of(&scenario);
    let requeued: Vec<&Event> = events
        .iter()
        .filter(|event| event.kind == EventKind::Requeued)
        .collect();
    assert_eq!(requeued.len(), 1);
    assert_eq!((requeued[0].car, requeued[0].at_ms), (Some(0), 20));
}

#[test]
fn acknowledging_an_alarm_changes_its_state_and_is_logged() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    assert_eq!(scenario.acknowledge_alarm(1), Err(AlarmError::NoSuchAlarm));
    scenario.fail_car(2).ok();
    let version = scenario.alarm_version();
    assert_eq!(scenario.acknowledge_alarm(1), Ok(()));
    assert_ne!(scenario.alarm_version(), version);
    assert_eq!(
        scenario.alarm_page(0).items[0].map(|a| a.state),
        Some(AlarmState::Acknowledged)
    );
    assert_eq!(scenario.acknowledge_alarm(1), Err(AlarmError::WrongState));
    assert_eq!(
        kinds(&events_of(&scenario)),
        vec![EventKind::AlarmRaised, EventKind::AlarmAcked]
    );
}

#[test]
fn the_version_changes_only_when_alarms_change() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    let version = scenario.alarm_version();
    scenario.step(10);
    scenario.press(call(3, Direction::Down), 11).ok();
    assert_eq!(scenario.alarm_version(), version);
}

#[test]
fn log_overflow_raises_an_alarm_that_survives_further_overflow() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    for round in 0..40 {
        scenario.step(round * 10);
        scenario.fail_car(0).ok();
        scenario.recover_car(0).ok();
    }
    let overflow = (0..2)
        .find_map(|offset| {
            scenario
                .alarm_page(offset * 8)
                .items
                .into_iter()
                .flatten()
                .find(|a| a.kind == AlarmKind::LogOverflow)
        })
        .expect("overflow alarm");
    assert_eq!((overflow.car, overflow.state), (None, AlarmState::Active));
    for round in 40..120 {
        scenario.step(round * 10);
        scenario.fail_car(0).ok();
        scenario.recover_car(0).ok();
    }
    let page = scenario.alarm_page(0);
    assert!(
        page.items
            .iter()
            .flatten()
            .any(|a| a.id == overflow.id && a.state == AlarmState::Active)
    );
}

#[test]
fn reset_clears_the_log_and_the_alarms() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.step(10);
    scenario.fail_car(0).ok();
    let version = scenario.alarm_version();
    scenario.reset();
    assert_eq!(events_of(&scenario), Vec::new());
    assert_eq!(scenario.alarm_page(0).total, 0);
    assert_ne!(scenario.alarm_version(), version);
}

#[test]
fn dispatcher_events_alone_can_overflow_the_log_and_raise_the_alarm() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    for round in 0..5_u64 {
        for tick in 1..=6 {
            scenario.step(round * 100 + tick * 10);
        }
        scenario.reload(round * 100 + 90);
    }
    let found = (0..2).any(|offset| {
        scenario
            .alarm_page(offset * 8)
            .items
            .into_iter()
            .flatten()
            .any(|alarm| alarm.kind == AlarmKind::LogOverflow)
    });
    assert!(found);
}

#[test]
fn the_acknowledgement_event_names_the_car_and_the_alarm() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    scenario.fail_car(2).ok();
    scenario.acknowledge_alarm(1).ok();
    let events = events_of(&scenario);
    assert_eq!(
        (events[1].kind, events[1].car, events[1].id),
        (EventKind::AlarmAcked, Some(2), 1)
    );
}

#[test]
fn restart_also_forgets_the_history() {
    let mut scenario = Scenario::new();
    scenario.restart(0);
    scenario.step(10);
    scenario.fail_car(0).ok();
    scenario.restart(100);
    assert_eq!(events_of(&scenario), Vec::new());
    assert_eq!(scenario.alarm_page(0).total, 0);
}

#[test]
fn only_an_active_critical_alarm_counts_for_the_red_led() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    assert!(!scenario.critical_alarm_active());
    scenario.fail_car(0).ok();
    assert!(scenario.critical_alarm_active());
    scenario.acknowledge_alarm(1).ok();
    assert!(!scenario.critical_alarm_active());
    scenario.recover_car(0).ok();
    assert!(!scenario.critical_alarm_active());
}

#[test]
fn a_log_overflow_alarm_is_not_critical() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    for round in 0..40 {
        scenario.step(round * 10);
        scenario.fail_car(0).ok();
        scenario.recover_car(0).ok();
    }
    assert!(!scenario.critical_alarm_active());
}

#[test]
fn a_second_failed_car_keeps_the_alarm_critical_after_the_first_is_acknowledged() {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    scenario.fail_car(0).ok();
    scenario.fail_car(1).ok();
    scenario.acknowledge_alarm(1).ok();
    assert!(scenario.critical_alarm_active());
}
