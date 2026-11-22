use core::num::NonZeroU32;

use core::cell::RefCell;
use cortex_m::peripheral::DWT;

use core::sync::atomic::Ordering;

use elevator_fw_logic::button::{Button, ButtonEvent};
use elevator_fw_logic::commands::{Reply, answer};
use elevator_fw_logic::progress::ProgressMonitor;
use elevator_fw_logic::scenario::{CARS, Scenario};
use elevator_fw_logic::{JitterReport, JitterStats, led_on};
use elevator_wire::protocol::{
    CommandResult, CommandStatus, Envelope, Message, SimAction, SimControl,
};
use embassy_stm32::gpio::{Input, Level, Output};
use embassy_sync::blocking_mutex::Mutex;
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

pub const TASK_CONTROL: usize = 0;
pub const MANDATORY_TASKS: usize = 1;

pub static PROGRESS: Mutex<CriticalSectionRawMutex, RefCell<ProgressMonitor<MANDATORY_TASKS>>> =
    Mutex::new(RefCell::new(ProgressMonitor::new()));

pub static BUTTON: Signal<CriticalSectionRawMutex, (ButtonEvent, bool)> = Signal::new();

pub static REPORT: Signal<CriticalSectionRawMutex, Reports> = Signal::new();

fn fresh(scenario: &mut Scenario, now_ms: u64) {
    if cfg!(feature = "load-test") {
        scenario.restart(now_ms);
    } else {
        scenario.reset();
    }
}

fn resets_run(envelope: &Envelope, reply: &CommandResult) -> bool {
    matches!(
        envelope.message,
        Message::SimControl(SimControl {
            action: SimAction::Reset,
            ..
        })
    ) && reply.status == CommandStatus::Applied
}

#[embassy_executor::task]
pub async fn run(mut led: Output<'static>, b1: Input<'static>, cycles_per_us: u32) {
    let period = Duration::from_micros(u64::from(CONTROL_PERIOD_US.get()));
    let mut stats = JitterStats::new(CONTROL_PERIOD_US);
    let period_cycles = NonZeroU32::new(CONTROL_PERIOD_US.get().saturating_mul(cycles_per_us))
        .unwrap_or(NonZeroU32::MIN);
    let mut step_stats = JitterStats::new(period_cycles);
    let mut scenario = Scenario::new();
    let mut button = Button::new();
    let start = Instant::now();
    fresh(&mut scenario, start.as_millis());
    let mut deadline = start + period;
    let mut report_at = start + REPORT_EVERY;
    let mut published_alarms = scenario.alarm_version().wrapping_add(1);
    loop {
        Timer::at(deadline).await;
        PROGRESS.lock(|monitor| monitor.borrow_mut().progress(TASK_CONTROL));
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
        if let Some(event) = button.update(deadline.as_millis(), b1.is_high()) {
            let failed = match event {
                ButtonEvent::Short => scenario.toggle_failure(0),
                ButtonEvent::Long => {
                    fresh(&mut scenario, deadline.as_millis());
                    crate::link::RUN_ID.fetch_add(1, Ordering::Relaxed);
                    false
                }
            };
            BUTTON.signal((event, failed));
        }
        while let Ok(envelope) = crate::link::COMMANDS.try_receive() {
            let reply = answer(
                crate::link::BOOT_ID.load(Ordering::Relaxed),
                crate::link::RUN_ID.load(Ordering::Relaxed),
                &envelope,
                &mut scenario,
                deadline.as_millis(),
            );
            match reply {
                Some(Reply::Result(result)) => {
                    if resets_run(&envelope, &result) {
                        crate::link::RUN_ID.fetch_add(1, Ordering::Relaxed);
                    }
                    let _ = crate::link::REPLIES.try_send(result);
                }
                Some(Reply::Log(chunk)) => {
                    let _ = crate::link::LOG_CHUNKS.try_send(chunk);
                }
                None => {}
            }
        }
        if scenario.alarm_version() != published_alarms {
            published_alarms = scenario.alarm_version();
            crate::link::ALARM_PAGES.signal([scenario.alarm_page(0), scenario.alarm_page(8)]);
        }
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
            crate::link::SNAPSHOTS.signal(core::array::from_fn::<_, CARS, _>(|car| {
                scenario.car_snapshot(car, deadline.as_millis())
            }));
            if cfg!(feature = "load-test") {
                scenario.reload(deadline.as_millis());
            }
            report_at += REPORT_EVERY;
        }
        deadline += period;
    }
}
