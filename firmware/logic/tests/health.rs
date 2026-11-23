use elevator_fw_logic::health::{
    OVERRUN_STREAK, OverrunWatch, RECOVERY_STREAK, STUCK_INPUT_MS, StuckInput, Verdict,
};

#[test]
fn a_released_input_is_never_stuck() {
    let mut input = StuckInput::new();
    for now in (0..60_000).step_by(100) {
        assert!(!input.update(now, false));
    }
}

#[test]
fn an_input_held_for_the_limit_is_stuck_and_release_clears_it() {
    let mut input = StuckInput::new();
    assert!(!input.update(1_000, true));
    assert!(!input.update(1_000 + STUCK_INPUT_MS - 1, true));
    assert!(input.update(1_000 + STUCK_INPUT_MS, true));
    assert!(input.update(1_000 + STUCK_INPUT_MS + 5_000, true));
    assert!(!input.update(30_000, false));
    assert!(!input.update(30_010, true));
}

#[test]
fn a_release_in_the_middle_restarts_the_timing() {
    let mut input = StuckInput::new();
    input.update(0, true);
    input.update(STUCK_INPUT_MS - 1, true);
    input.update(STUCK_INPUT_MS, false);
    assert!(!input.update(STUCK_INPUT_MS + 1, true));
    assert!(!input.update(STUCK_INPUT_MS + STUCK_INPUT_MS, true));
    assert!(input.update(STUCK_INPUT_MS + 1 + STUCK_INPUT_MS, true));
}

#[test]
fn time_going_backwards_is_not_stuck() {
    let mut input = StuckInput::new();
    input.update(5_000, true);
    assert!(!input.update(0, true));
}

#[test]
fn consecutive_late_ticks_make_a_fault_but_isolated_ones_do_not() {
    let mut watch = OverrunWatch::new();
    for _ in 0..OVERRUN_STREAK - 1 {
        assert!(!watch.update(true).fault);
    }
    assert!(!watch.update(false).fault);
    for _ in 0..OVERRUN_STREAK - 1 {
        assert!(!watch.update(true).fault);
    }
    assert!(watch.update(true).fault);
}

#[test]
fn the_fault_persists_while_ticks_stay_late_and_recovers_after_a_long_clean_run() {
    let mut watch = OverrunWatch::new();
    for _ in 0..OVERRUN_STREAK {
        watch.update(true);
    }
    assert!(watch.update(true).fault);
    assert!(!watch.update(true).recovered);
    for i in 0..RECOVERY_STREAK - 1 {
        let verdict = watch.update(false);
        assert!(!verdict.recovered, "tick {i}");
        assert!(!verdict.fault);
    }
    assert_eq!(
        watch.update(false),
        Verdict {
            fault: false,
            recovered: true
        }
    );
}

#[test]
fn one_late_tick_resets_the_recovery_streak() {
    let mut watch = OverrunWatch::new();
    for _ in 0..RECOVERY_STREAK - 1 {
        watch.update(false);
    }
    watch.update(true);
    assert!(!watch.update(false).recovered);
}
