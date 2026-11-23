pub const STUCK_INPUT_MS: u64 = 10_000;
pub const OVERRUN_STREAK: u32 = 5;
pub const RECOVERY_STREAK: u32 = 100;

#[derive(Clone, Copy, Debug, Default)]
pub struct StuckInput {
    pressed_since: Option<u64>,
}

impl StuckInput {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pressed_since: None,
        }
    }

    pub fn update(&mut self, now_ms: u64, pressed: bool) -> bool {
        if !pressed {
            self.pressed_since = None;
            return false;
        }
        let since = *self.pressed_since.get_or_insert(now_ms);
        now_ms.saturating_sub(since) >= STUCK_INPUT_MS
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    pub fault: bool,
    pub recovered: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OverrunWatch {
    late_streak: u32,
    on_time_streak: u32,
}

impl OverrunWatch {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            late_streak: 0,
            on_time_streak: 0,
        }
    }

    pub fn update(&mut self, late: bool) -> Verdict {
        if late {
            self.late_streak = self.late_streak.saturating_add(1);
            self.on_time_streak = 0;
        } else {
            self.on_time_streak = self.on_time_streak.saturating_add(1);
            self.late_streak = 0;
        }
        Verdict {
            fault: self.late_streak >= OVERRUN_STREAK,
            recovered: self.on_time_streak >= RECOVERY_STREAK,
        }
    }
}
