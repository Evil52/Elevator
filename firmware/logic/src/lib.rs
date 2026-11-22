#![cfg_attr(not(test), no_std)]

use core::num::NonZeroU32;

pub mod alarms;
pub mod button;
pub mod commands;
pub mod events;
pub mod progress;
pub mod reset;
pub mod scenario;
pub mod telemetry;

#[must_use]
pub fn led_on(now_ms: u64, period_ms: NonZeroU32) -> bool {
    let period = u64::from(period_ms.get());
    (now_ms % period) * 2 < period
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JitterReport {
    pub count: u32,
    pub min_us: u32,
    pub mean_us: u32,
    pub max_us: u32,
    pub overruns: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct JitterStats {
    period_us: NonZeroU32,
    count: u32,
    min_us: u32,
    max_us: u32,
    sum_us: u64,
    overruns: u32,
}

impl JitterStats {
    #[must_use]
    pub const fn new(period_us: NonZeroU32) -> Self {
        Self {
            period_us,
            count: 0,
            min_us: 0,
            max_us: 0,
            sum_us: 0,
            overruns: 0,
        }
    }

    pub fn record(&mut self, late_us: u32) {
        if self.count == 0 {
            self.min_us = late_us;
            self.max_us = late_us;
        } else {
            self.min_us = self.min_us.min(late_us);
            self.max_us = self.max_us.max(late_us);
        }
        self.count = self.count.saturating_add(1);
        self.sum_us = self.sum_us.saturating_add(u64::from(late_us));
        if late_us >= self.period_us.get() {
            self.overruns = self.overruns.saturating_add(1);
        }
    }

    pub fn take(&mut self) -> Option<JitterReport> {
        if self.count == 0 {
            return None;
        }
        let mean_us = u32::try_from(self.sum_us / u64::from(self.count)).unwrap_or(u32::MAX);
        let report = JitterReport {
            count: self.count,
            min_us: self.min_us,
            mean_us,
            max_us: self.max_us,
            overruns: self.overruns,
        };
        *self = Self::new(self.period_us);
        Some(report)
    }
}
