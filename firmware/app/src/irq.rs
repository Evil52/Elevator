#![allow(unsafe_code)]

use embassy_executor::InterruptExecutor;
use embassy_stm32::interrupt;

pub static CONTROL_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn UART4() {
    unsafe { CONTROL_EXECUTOR.on_interrupt() }
}
