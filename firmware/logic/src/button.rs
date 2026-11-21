pub const DEBOUNCE_MS: u64 = 20;
pub const LONG_PRESS_MS: u64 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
    Short,
    Long,
}

#[derive(Clone, Copy, Debug)]
pub struct Button {
    stable: bool,
    candidate: bool,
    candidate_since: u64,
    pressed_since: u64,
    long_sent: bool,
}

impl Button {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stable: false,
            candidate: false,
            candidate_since: 0,
            pressed_since: 0,
            long_sent: false,
        }
    }

    pub fn update(&mut self, now_ms: u64, pressed: bool) -> Option<ButtonEvent> {
        if pressed != self.candidate {
            self.candidate = pressed;
            self.candidate_since = now_ms;
        }
        let settled = now_ms.saturating_sub(self.candidate_since) >= DEBOUNCE_MS;
        if settled && self.candidate != self.stable {
            self.stable = self.candidate;
            if self.stable {
                self.pressed_since = self.candidate_since;
                self.long_sent = false;
                return None;
            }
            return (!self.long_sent).then_some(ButtonEvent::Short);
        }
        if self.stable
            && !self.long_sent
            && now_ms.saturating_sub(self.pressed_since) >= LONG_PRESS_MS
        {
            self.long_sent = true;
            return Some(ButtonEvent::Long);
        }
        None
    }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}
