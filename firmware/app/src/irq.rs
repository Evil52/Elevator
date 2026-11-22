#![allow(unsafe_code)]

use embassy_executor::InterruptExecutor;
use embassy_stm32::{bind_interrupts, dma, interrupt, peripherals, rng};

pub static CONTROL_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

bind_interrupts!(pub struct Irqs {
    HASH_RNG => rng::InterruptHandler<peripherals::RNG>;
    DMA1_STREAM3 => dma::InterruptHandler<peripherals::DMA1_CH3>;
});

#[interrupt]
unsafe fn UART4() {
    unsafe { CONTROL_EXECUTOR.on_interrupt() }
}
