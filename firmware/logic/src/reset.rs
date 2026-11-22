#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResetFlags {
    pub low_power: bool,
    pub window_watchdog: bool,
    pub independent_watchdog: bool,
    pub software: bool,
    pub power_on: bool,
    pub brown_out: bool,
    pub pin: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetCause {
    IndependentWatchdog,
    WindowWatchdog,
    LowPower,
    Software,
    PowerOn,
    BrownOut,
    Pin,
    Unknown,
}

#[must_use]
pub fn reset_cause(flags: ResetFlags) -> ResetCause {
    if flags.independent_watchdog {
        ResetCause::IndependentWatchdog
    } else if flags.window_watchdog {
        ResetCause::WindowWatchdog
    } else if flags.low_power {
        ResetCause::LowPower
    } else if flags.software {
        ResetCause::Software
    } else if flags.power_on {
        ResetCause::PowerOn
    } else if flags.brown_out {
        ResetCause::BrownOut
    } else if flags.pin {
        ResetCause::Pin
    } else {
        ResetCause::Unknown
    }
}
