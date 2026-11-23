use elevator_fw_logic::supervisor::{
    INPUTS_TIMEOUT_MS, Inputs, RECOVERY_HOLD_MS, SELF_CHECK_TIMEOUT_MS, State, Supervisor,
};

fn ok() -> Inputs {
    Inputs {
        self_check: Some(true),
        inputs_ready: true,
        fault: false,
        recovered: false,
    }
}

fn running() -> Supervisor {
    let mut supervisor = Supervisor::new();
    supervisor.step(0, ok());
    supervisor.step(10, ok());
    supervisor.step(20, ok());
    supervisor
}

#[test]
fn it_starts_in_boot_with_outputs_off() {
    let supervisor = Supervisor::new();
    assert_eq!(supervisor.state(), State::Boot);
    assert!(!supervisor.outputs_enabled());
}

#[test]
fn it_walks_boot_self_check_inputs_ready_running_one_step_at_a_time() {
    let mut supervisor = Supervisor::new();
    assert_eq!(supervisor.step(0, ok()), State::SelfCheck);
    assert_eq!(supervisor.step(10, ok()), State::InputsReady);
    assert_eq!(supervisor.step(20, ok()), State::Running);
    assert!(supervisor.outputs_enabled());
}

#[test]
fn outputs_are_on_only_while_running() {
    let mut supervisor = Supervisor::new();
    for (now, expected) in [(0, false), (10, false), (20, true)] {
        supervisor.step(now, ok());
        assert_eq!(supervisor.outputs_enabled(), expected, "t={now}");
    }
    supervisor.step(
        30,
        Inputs {
            fault: true,
            ..ok()
        },
    );
    assert!(!supervisor.outputs_enabled());
}

#[test]
fn a_failed_self_check_goes_to_fault() {
    let mut supervisor = Supervisor::new();
    supervisor.step(0, ok());
    let failed = Inputs {
        self_check: Some(false),
        ..ok()
    };
    assert_eq!(supervisor.step(10, failed), State::Fault);
}

#[test]
fn a_self_check_that_never_finishes_times_out_into_fault() {
    let mut supervisor = Supervisor::new();
    let pending = Inputs {
        self_check: None,
        ..ok()
    };
    supervisor.step(100, pending);
    assert_eq!(
        supervisor.step(100 + SELF_CHECK_TIMEOUT_MS - 1, pending),
        State::SelfCheck
    );
    assert_eq!(
        supervisor.step(100 + SELF_CHECK_TIMEOUT_MS, pending),
        State::Fault
    );
}

#[test]
fn inputs_that_never_get_ready_time_out_into_fault() {
    let mut supervisor = Supervisor::new();
    supervisor.step(0, ok());
    supervisor.step(10, ok());
    assert_eq!(supervisor.state(), State::InputsReady);
    let waiting = Inputs {
        inputs_ready: false,
        ..ok()
    };
    assert_eq!(
        supervisor.step(10 + INPUTS_TIMEOUT_MS - 1, waiting),
        State::InputsReady
    );
    assert_eq!(
        supervisor.step(10 + INPUTS_TIMEOUT_MS, waiting),
        State::Fault
    );
}

#[test]
fn a_fault_input_stops_every_state_before_running() {
    for steps in 0..3 {
        let mut supervisor = Supervisor::new();
        for i in 0..steps {
            supervisor.step(i * 10, ok());
        }
        let state = supervisor.step(
            steps * 10,
            Inputs {
                fault: true,
                ..ok()
            },
        );
        if steps >= 1 {
            assert_eq!(state, State::Fault, "after {steps} steps");
        }
    }
}

#[test]
fn running_goes_to_fault_on_a_fault_or_lost_inputs() {
    let mut supervisor = running();
    assert_eq!(
        supervisor.step(
            30,
            Inputs {
                fault: true,
                ..ok()
            }
        ),
        State::Fault
    );
    let mut supervisor = running();
    assert_eq!(
        supervisor.step(
            30,
            Inputs {
                inputs_ready: false,
                ..ok()
            }
        ),
        State::Fault
    );
}

#[test]
fn a_fault_waits_for_recovery_then_holds_before_checking_inputs_again() {
    let mut supervisor = running();
    supervisor.step(
        30,
        Inputs {
            fault: true,
            ..ok()
        },
    );
    assert_eq!(supervisor.step(40, ok()), State::Fault);
    assert_eq!(
        supervisor.step(
            50,
            Inputs {
                recovered: true,
                ..ok()
            }
        ),
        State::Recovery
    );
    assert!(!supervisor.outputs_enabled());
    assert_eq!(
        supervisor.step(50 + RECOVERY_HOLD_MS - 1, ok()),
        State::Recovery
    );
    assert_eq!(
        supervisor.step(50 + RECOVERY_HOLD_MS, ok()),
        State::InputsReady
    );
    assert_eq!(
        supervisor.step(50 + RECOVERY_HOLD_MS + 10, ok()),
        State::Running
    );
}

#[test]
fn a_new_fault_during_recovery_returns_to_fault() {
    let mut supervisor = running();
    supervisor.step(
        30,
        Inputs {
            fault: true,
            ..ok()
        },
    );
    supervisor.step(
        40,
        Inputs {
            recovered: true,
            ..ok()
        },
    );
    assert_eq!(supervisor.state(), State::Recovery);
    assert_eq!(
        supervisor.step(
            100,
            Inputs {
                fault: true,
                ..ok()
            }
        ),
        State::Fault
    );
}

#[test]
fn recovery_does_not_finish_while_inputs_are_not_ready() {
    let mut supervisor = running();
    supervisor.step(
        30,
        Inputs {
            fault: true,
            ..ok()
        },
    );
    supervisor.step(
        40,
        Inputs {
            recovered: true,
            ..ok()
        },
    );
    let bad = Inputs {
        inputs_ready: false,
        ..ok()
    };
    assert_eq!(supervisor.step(40 + RECOVERY_HOLD_MS, bad), State::Recovery);
}

#[test]
fn time_going_backwards_never_underflows() {
    let mut supervisor = Supervisor::new();
    supervisor.step(1_000, ok());
    assert_eq!(
        supervisor.step(
            0,
            Inputs {
                self_check: None,
                ..ok()
            }
        ),
        State::SelfCheck
    );
}

#[test]
fn a_failed_self_check_is_latched_and_never_recovers_by_itself() {
    let mut supervisor = Supervisor::new();
    supervisor.step(0, ok());
    let failed = Inputs {
        self_check: Some(false),
        ..ok()
    };
    assert_eq!(supervisor.step(10, failed), State::Fault);
    for now in (20..10_000).step_by(100) {
        assert_eq!(
            supervisor.step(
                now,
                Inputs {
                    recovered: true,
                    ..ok()
                }
            ),
            State::Fault,
            "t={now}"
        );
    }
    assert!(!supervisor.outputs_enabled());
}

#[test]
fn a_self_check_timeout_is_latched_too() {
    let mut supervisor = Supervisor::new();
    let pending = Inputs {
        self_check: None,
        ..ok()
    };
    supervisor.step(0, pending);
    assert_eq!(
        supervisor.step(SELF_CHECK_TIMEOUT_MS, pending),
        State::Fault
    );
    assert_eq!(
        supervisor.step(
            SELF_CHECK_TIMEOUT_MS + 10,
            Inputs {
                recovered: true,
                ..ok()
            }
        ),
        State::Fault
    );
}

#[test]
fn a_fault_that_came_from_running_is_not_latched() {
    let mut supervisor = running();
    supervisor.step(
        30,
        Inputs {
            fault: true,
            ..ok()
        },
    );
    assert_eq!(
        supervisor.step(
            40,
            Inputs {
                recovered: true,
                ..ok()
            }
        ),
        State::Recovery
    );
}

#[test]
fn an_input_timeout_fault_can_recover() {
    let mut supervisor = Supervisor::new();
    supervisor.step(0, ok());
    supervisor.step(10, ok());
    let waiting = Inputs {
        inputs_ready: false,
        ..ok()
    };
    assert_eq!(
        supervisor.step(10 + INPUTS_TIMEOUT_MS, waiting),
        State::Fault
    );
    assert_eq!(
        supervisor.step(
            20 + INPUTS_TIMEOUT_MS,
            Inputs {
                recovered: true,
                ..ok()
            }
        ),
        State::Recovery
    );
}
