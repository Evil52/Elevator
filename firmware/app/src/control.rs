use core::num::NonZeroU32;

use cortex_m::peripheral::DWT;
use elevator_fw_logic::scenario::Scenario;
use elevator_fw_logic::{JitterReport, JitterStats, led_on};
use embassy_stm32::gpio::{Level, Output};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};

pub const CONTROL_PERIOD_US: NonZeroU32 = match NonZeroU32::new(10_000) {
    Some(value) => value,
    None => NonZeroU32::MIN,
};
pub const BLINK_PERIOD_MS: NonZeroU32 = match NonZeroU32::new(1_000) {
    Some(value) => value,
    None => NonZeroU32::MIN,
};
const REPORT_EVERY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy)]
pub struct Reports {
    pub jitter: JitterReport,
    pub step_cycles: JitterReport,
}

pub static REPORT: Signal<CriticalSectionRawMutex, Reports> = Signal::new();

#[embassy_executor::task]
pub async fn run(mut led: Output<'static>, cycles_per_us: u32) {
    let period = Duration::from_micros(u64::from(CONTROL_PERIOD_US.get()));
    let mut stats = JitterStats::new(CONTROL_PERIOD_US);
    let period_cycles = NonZeroU32::new(CONTROL_PERIOD_US.get().saturating_mul(cycles_per_us))
        .unwrap_or(NonZeroU32::MIN);
    let mut step_stats = JitterStats::new(period_cycles);
    let mut scenario = Scenario::new();
    let start = Instant::now();
    scenario.restart(start.as_millis());
    let mut deadline = start + period;
    let mut report_at = start + REPORT_EVERY;
    loop {
        Timer::at(deadline).await;
        let now = Instant::now();
        let late_us = now
            .checked_duration_since(deadline)
            .map_or(0, |late| late.as_micros());
        stats.record(u32::try_from(late_us).unwrap_or(u32::MAX));
        led.set_level(if led_on(deadline.as_millis(), BLINK_PERIOD_MS) {
            Level::High
        } else {
            Level::Low
        });
        let began = DWT::cycle_count();
        scenario.step(deadline.as_millis());
        step_stats.record(DWT::cycle_count().wrapping_sub(began));
        if now >= report_at {
            if let (Some(jitter), Some(step_cycles)) = (stats.take(), step_stats.take()) {
                REPORT.signal(Reports {
                    jitter,
                    step_cycles,
                });
            }
            scenario.restart(deadline.as_millis());
            report_at += REPORT_EVERY;
        }
        deadline += period;
    }
}
