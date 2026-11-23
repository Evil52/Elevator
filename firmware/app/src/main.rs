#![no_std]
#![no_main]

mod control;
mod irq;
mod link;

use defmt::{error, info, warn};
use defmt_rtt as _;
use elevator_fw_logic::button::ButtonEvent;
use elevator_fw_logic::progress::Verdict;
use elevator_fw_logic::reset::{ResetCause, ResetFlags, reset_cause};
use elevator_fw_logic::supervisor::State;
use elevator_fw_logic::telemetry::Telemetry;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::interrupt::{self, InterruptExt, Priority};
use embassy_stm32::peripherals::IWDG;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource,
    Sysclk,
};
use embassy_stm32::rng::Rng;
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{Config as UartConfig, Uart};
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_time::{Duration, Ticker};
use panic_probe as _;

fn clock_config() -> embassy_stm32::Config {
    let mut config = embassy_stm32::Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz::mhz(8),
        mode: HseMode::Bypass,
    });
    config.rcc.pll_src = PllSource::HSE;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV4,
        mul: PllMul::MUL180,
        divp: Some(PllPDiv::DIV2),
        divq: Some(PllQDiv::DIV8),
        divr: None,
    });
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV4;
    config.rcc.apb2_pre = APBPrescaler::DIV2;
    config
}

#[embassy_executor::task]
async fn report(cycles_per_us: u32) {
    loop {
        let r = control::REPORT.wait().await;
        info!(
            "jitter: n={} min={}us mean={}us max={}us overruns={}",
            r.jitter.count, r.jitter.min_us, r.jitter.mean_us, r.jitter.max_us, r.jitter.overruns
        );
        info!(
            "step: min={} mean={} max={} cycles, max {} us, overruns={}",
            r.step_cycles.min_us,
            r.step_cycles.mean_us,
            r.step_cycles.max_us,
            r.step_cycles.max_us / cycles_per_us.max(1),
            r.step_cycles.overruns
        );
    }
}

const WATCHDOG_TIMEOUT_US: u32 = 1_000_000;
const WATCHDOG_CHECK: Duration = Duration::from_millis(100);

fn read_reset_cause() -> ResetCause {
    let rcc = embassy_stm32::pac::RCC;
    let csr = rcc.csr().read();
    let cause = reset_cause(ResetFlags {
        low_power: csr.lpwrrstf(),
        window_watchdog: csr.wwdgrstf(),
        independent_watchdog: csr.wdgrstf(),
        software: csr.sftrstf(),
        power_on: csr.porrstf(),
        brown_out: csr.borrstf(),
        pin: csr.padrstf(),
    });
    rcc.csr().modify(|w| w.set_rmvf(true));
    cause
}

fn enable_flash_accelerator() {
    embassy_stm32::pac::FLASH.acr().modify(|w| {
        w.set_prften(true);
        w.set_icen(true);
        w.set_dcen(true);
    });
}

const fn state_name(state: State) -> &'static str {
    match state {
        State::Boot => "boot",
        State::SelfCheck => "self check",
        State::InputsReady => "inputs ready",
        State::Running => "running",
        State::Fault => "fault",
        State::Recovery => "recovery",
    }
}

#[embassy_executor::task]
async fn report_link() {
    loop {
        let up = control::LINK.wait().await;
        info!("gateway link: {}", if up { "up" } else { "down" });
    }
}

#[embassy_executor::task]
async fn report_state() {
    loop {
        let state = control::SUPERVISOR.wait().await;
        info!("supervisor: {}", state_name(state));
    }
}

const fn cause_name(cause: ResetCause) -> &'static str {
    match cause {
        ResetCause::IndependentWatchdog => "independent watchdog",
        ResetCause::WindowWatchdog => "window watchdog",
        ResetCause::LowPower => "low power",
        ResetCause::Software => "software",
        ResetCause::PowerOn => "power on",
        ResetCause::BrownOut => "brown out",
        ResetCause::Pin => "pin",
        ResetCause::Unknown => "unknown",
    }
}

fn fold_uid(uid: [u8; 12]) -> u32 {
    let (words, _) = uid.as_chunks::<4>();
    words
        .iter()
        .fold(0, |acc, word| acc ^ u32::from_le_bytes(*word))
}

#[embassy_executor::task]
async fn watchdog(mut iwdg: IndependentWatchdog<'static, IWDG>) {
    iwdg.unleash();
    let mut ticker = Ticker::every(WATCHDOG_CHECK);
    loop {
        ticker.next().await;
        match control::PROGRESS.lock(|monitor| monitor.borrow_mut().check()) {
            Verdict::Feed => iwdg.pet(),
            Verdict::Hold { missing } => warn!("watchdog hold, missing tasks mask {}", missing),
        }
    }
}

#[embassy_executor::task]
async fn report_button() {
    loop {
        match control::BUTTON.wait().await {
            (ButtonEvent::Short, failed) => info!("b1: short press, car 0 lock failure {}", failed),
            (ButtonEvent::Long, _) => info!("b1: long press, scenario reset"),
        }
    }
}

#[embassy_executor::main]
async fn main(low_spawner: Spawner) {
    let cause = read_reset_cause();
    let p = embassy_stm32::init(clock_config());
    enable_flash_accelerator();
    let clocks = embassy_stm32::rcc::clocks(&p.RCC);
    info!(
        "boot: sys={} hclk={} pclk1_tim={} control period {} us",
        clocks.sys,
        clocks.hclk1,
        clocks.pclk1_tim,
        control::CONTROL_PERIOD_US.get()
    );
    let cycles_per_us = clocks.hclk1.to_hertz().map_or(180, |hz| hz.0 / 1_000_000);
    let Some(mut core) = cortex_m::Peripherals::take() else {
        error!("core peripherals already taken");
        return;
    };
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();
    let led = Output::new(p.PB0, Level::Low, Speed::Low);
    let led2 = Output::new(p.PB7, Level::Low, Speed::Low);
    let led3 = Output::new(p.PB14, Level::Low, Speed::Low);
    info!("reset cause: {}", cause_name(cause));
    let mut rng = Rng::new(p.RNG, irq::Irqs);
    let mut random = [0_u8; 4];
    let rng_ok = rng.async_fill_bytes(&mut random).await.is_ok();
    let boot_id = if rng_ok {
        u32::from_le_bytes(random)
    } else {
        0
    };
    let device_id = fold_uid(embassy_stm32::uid::uid());
    info!("boot id {} device id {}", boot_id, device_id);
    link::BOOT_ID.store(boot_id, core::sync::atomic::Ordering::Relaxed);
    let mut uart_config = UartConfig::default();
    uart_config.eager_reads = Some(1);
    let uart = Uart::new(
        p.USART3,
        p.PD9,
        p.PD8,
        p.DMA1_CH3,
        p.DMA1_CH1,
        irq::Irqs,
        uart_config,
    );
    let b1 = Input::new(p.PC13, Pull::None);
    interrupt::UART4.set_priority(Priority::P6);
    let high_spawner = irq::CONTROL_EXECUTOR.start(interrupt::UART4);
    let clock_ok = clocks.sys.to_hertz().is_some_and(|hz| hz.0 == 180_000_000);
    let self_check_ok = rng_ok && clock_ok && uart.is_ok();
    info!(
        "self check: rng {} clock {} uart {}",
        rng_ok,
        clock_ok,
        uart.is_ok()
    );
    if let Ok(token) = control::run(
        control::Leds {
            green: led,
            blue: led2,
            red: led3,
        },
        b1,
        cycles_per_us,
        self_check_ok,
    ) {
        high_spawner.spawn(token);
    } else {
        error!("control task did not start");
    }
    if let Ok(uart) = uart {
        let (tx, rx) = uart.split();
        if let Ok(token) = link::transmit(tx, Telemetry::new(device_id, boot_id), cause) {
            low_spawner.spawn(token);
        } else {
            error!("transmit task did not start");
        }
        if let Ok(token) = link::receive(rx) {
            low_spawner.spawn(token);
        } else {
            error!("receive task did not start");
        }
    } else {
        error!("usart3 did not start");
    }
    if let Ok(token) = watchdog(IndependentWatchdog::new(p.IWDG, WATCHDOG_TIMEOUT_US)) {
        low_spawner.spawn(token);
    } else {
        error!("watchdog task did not start");
    }
    if let Ok(token) = report_link() {
        low_spawner.spawn(token);
    } else {
        error!("link report task did not start");
    }
    if let Ok(token) = report_state() {
        low_spawner.spawn(token);
    } else {
        error!("state report task did not start");
    }
    if let Ok(token) = report_button() {
        low_spawner.spawn(token);
    } else {
        error!("button report task did not start");
    }
    if let Ok(token) = report(cycles_per_us) {
        low_spawner.spawn(token);
    } else {
        error!("report task did not start");
    }
}
