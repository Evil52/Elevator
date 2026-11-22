use elevator_fw_logic::reset::{ResetCause, ResetFlags, reset_cause};

#[test]
fn no_flags_means_unknown() {
    assert_eq!(reset_cause(ResetFlags::default()), ResetCause::Unknown);
}

#[test]
fn every_single_flag_maps_to_its_cause() {
    let cases = [
        (
            ResetFlags {
                independent_watchdog: true,
                ..ResetFlags::default()
            },
            ResetCause::IndependentWatchdog,
        ),
        (
            ResetFlags {
                window_watchdog: true,
                ..ResetFlags::default()
            },
            ResetCause::WindowWatchdog,
        ),
        (
            ResetFlags {
                low_power: true,
                ..ResetFlags::default()
            },
            ResetCause::LowPower,
        ),
        (
            ResetFlags {
                software: true,
                ..ResetFlags::default()
            },
            ResetCause::Software,
        ),
        (
            ResetFlags {
                power_on: true,
                ..ResetFlags::default()
            },
            ResetCause::PowerOn,
        ),
        (
            ResetFlags {
                brown_out: true,
                ..ResetFlags::default()
            },
            ResetCause::BrownOut,
        ),
        (
            ResetFlags {
                pin: true,
                ..ResetFlags::default()
            },
            ResetCause::Pin,
        ),
    ];
    for (flags, cause) in cases {
        assert_eq!(reset_cause(flags), cause, "{flags:?}");
    }
}

#[test]
fn the_pin_flag_that_accompanies_other_resets_does_not_hide_them() {
    let with_pin = |flags: ResetFlags| ResetFlags { pin: true, ..flags };
    assert_eq!(
        reset_cause(with_pin(ResetFlags {
            independent_watchdog: true,
            ..ResetFlags::default()
        })),
        ResetCause::IndependentWatchdog
    );
    assert_eq!(
        reset_cause(with_pin(ResetFlags {
            software: true,
            ..ResetFlags::default()
        })),
        ResetCause::Software
    );
}

#[test]
fn power_on_sets_the_brown_out_flag_too_and_is_reported_as_power_on() {
    let flags = ResetFlags {
        power_on: true,
        brown_out: true,
        pin: true,
        ..ResetFlags::default()
    };
    assert_eq!(reset_cause(flags), ResetCause::PowerOn);
    let brown_out_only = ResetFlags {
        brown_out: true,
        pin: true,
        ..ResetFlags::default()
    };
    assert_eq!(reset_cause(brown_out_only), ResetCause::BrownOut);
}

#[test]
fn causes_follow_the_documented_priority() {
    let all = ResetFlags {
        low_power: true,
        window_watchdog: true,
        independent_watchdog: true,
        software: true,
        power_on: true,
        brown_out: true,
        pin: true,
    };
    assert_eq!(reset_cause(all), ResetCause::IndependentWatchdog);
    let no_iwdg = ResetFlags {
        independent_watchdog: false,
        ..all
    };
    assert_eq!(reset_cause(no_iwdg), ResetCause::WindowWatchdog);
    let no_window = ResetFlags {
        window_watchdog: false,
        ..no_iwdg
    };
    assert_eq!(reset_cause(no_window), ResetCause::LowPower);
    let no_low_power = ResetFlags {
        low_power: false,
        ..no_window
    };
    assert_eq!(reset_cause(no_low_power), ResetCause::Software);
    let no_software = ResetFlags {
        software: false,
        ..no_low_power
    };
    assert_eq!(reset_cause(no_software), ResetCause::PowerOn);
}
