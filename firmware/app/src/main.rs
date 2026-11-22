#![no_std]
#![no_main]

mod control;
mod irq;

use defmt::{error, info};
use defmt_rtt as _;
use elevator_fw_logic::button::ButtonEvent;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::interrupt::{self, InterruptExt, Priority};
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllSource, Sysclk,
};
use embassy_stm32::time::Hertz;
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
        divq: None,
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
    let p = embassy_stm32::init(clock_config());
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
    let b1 = Input::new(p.PC13, Pull::None);
    interrupt::UART4.set_priority(Priority::P6);
    let high_spawner = irq::CONTROL_EXECUTOR.start(interrupt::UART4);
    if let Ok(token) = control::run(led, b1, cycles_per_us) {
        high_spawner.spawn(token);
    } else {
        error!("control task did not start");
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
