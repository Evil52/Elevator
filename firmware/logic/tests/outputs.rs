use elevator_fw_logic::outputs::led3_on;
use elevator_fw_logic::supervisor::State;

#[test]
fn the_red_led_follows_a_critical_alarm_only_while_running() {
    assert!(led3_on(State::Running, true));
    assert!(!led3_on(State::Running, false));
}

#[test]
fn the_red_led_shows_a_fault_and_a_recovery_even_without_an_alarm() {
    assert!(led3_on(State::Fault, false));
    assert!(led3_on(State::Recovery, false));
}

#[test]
fn the_red_led_stays_dark_before_the_outputs_are_allowed() {
    for state in [State::Boot, State::SelfCheck, State::InputsReady] {
        assert!(!led3_on(state, true), "{state:?}");
        assert!(!led3_on(state, false), "{state:?}");
    }
}

#[test]
fn led2_follows_the_link_once_the_outputs_are_allowed() {
    use elevator_fw_logic::outputs::led2_on;
    for state in [State::Running, State::Fault, State::Recovery] {
        assert!(led2_on(state, true), "{state:?}");
        assert!(!led2_on(state, false), "{state:?}");
    }
}

#[test]
fn led2_stays_dark_before_the_outputs_are_allowed() {
    use elevator_fw_logic::outputs::led2_on;
    for state in [State::Boot, State::SelfCheck, State::InputsReady] {
        assert!(!led2_on(state, true), "{state:?}");
    }
}
