use elevator_wire::protocol::{Envelope, Message};

pub const LINK_TIMEOUT_MS: u64 = 3000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkMonitor {
    seen: u32,
    heard_at: Option<u64>,
}

impl LinkMonitor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seen: 0,
            heard_at: None,
        }
    }

    pub fn update(&mut self, now_ms: u64, frames: u32) -> bool {
        if frames != self.seen {
            self.seen = frames;
            self.heard_at = Some(now_ms);
        }
        self.heard_at
            .is_some_and(|heard| now_ms.saturating_sub(heard) < LINK_TIMEOUT_MS)
    }
}

#[must_use]
pub const fn needs_control(envelope: &Envelope) -> bool {
    !matches!(envelope.message, Message::Ping(_))
}
