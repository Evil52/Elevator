use core::sync::atomic::{AtomicU32, Ordering};

use elevator_fw_logic::reset::ResetCause;
use elevator_fw_logic::scenario::CARS;
use elevator_fw_logic::telemetry::Telemetry;
use elevator_wire::protocol::{CarSnapshot, CommandResult, Envelope, decode_message};
use elevator_wire::{Decoder, MAX_FRAME, MAX_PAYLOAD};
use embassy_futures::select::{Either, select};
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{UartRx, UartTx};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;

pub static SNAPSHOTS: Signal<CriticalSectionRawMutex, [Option<CarSnapshot>; CARS]> = Signal::new();
pub static COMMANDS: Channel<CriticalSectionRawMutex, Envelope, 4> = Channel::new();
pub static REPLIES: Channel<CriticalSectionRawMutex, CommandResult, 4> = Channel::new();
pub static BOOT_ID: AtomicU32 = AtomicU32::new(0);
pub static RUN_ID: AtomicU32 = AtomicU32::new(1);

#[embassy_executor::task]
pub async fn transmit(mut tx: UartTx<'static, Async>, mut telemetry: Telemetry, cause: ResetCause) {
    let mut frame = [0_u8; MAX_FRAME];
    if let Ok(length) = telemetry.hello(cause, &mut frame) {
        let _ = tx.write(&frame[..length]).await;
    }
    loop {
        let event = select(SNAPSHOTS.wait(), REPLIES.receive()).await;
        telemetry.set_run(RUN_ID.load(Ordering::Relaxed));
        match event {
            Either::First(snapshots) => {
                for snapshot in snapshots.into_iter().flatten() {
                    if let Ok(length) = telemetry.car_snapshot(snapshot, &mut frame) {
                        let _ = tx.write(&frame[..length]).await;
                    }
                }
            }
            Either::Second(result) => {
                if let Ok(length) = telemetry.command_result(result, &mut frame) {
                    let _ = tx.write(&frame[..length]).await;
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn receive(rx: UartRx<'static, Async>) {
    let mut ring = [0_u8; 128];
    let mut rx = rx.into_ring_buffered(&mut ring);
    rx.start_uart();
    let mut chunk = [0_u8; 64];
    let mut payload = [0_u8; MAX_PAYLOAD];
    let mut decoder = Decoder::new();
    loop {
        let Ok(count) = rx.read(&mut chunk).await else {
            decoder = Decoder::new();
            continue;
        };
        for &byte in &chunk[..count] {
            let Some(Ok((_, length))) = decoder.push(byte, &mut payload) else {
                continue;
            };
            if let Ok(envelope) = decode_message(&payload[..length]) {
                let _ = COMMANDS.try_send(envelope);
            }
        }
    }
}
