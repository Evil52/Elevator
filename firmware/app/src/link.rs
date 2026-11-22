use elevator_fw_logic::reset::ResetCause;
use elevator_fw_logic::scenario::CARS;
use elevator_fw_logic::telemetry::Telemetry;
use elevator_wire::MAX_FRAME;
use elevator_wire::protocol::CarSnapshot;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::UartTx;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

pub static SNAPSHOTS: Signal<CriticalSectionRawMutex, [Option<CarSnapshot>; CARS]> = Signal::new();
pub static NEW_RUN: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn run(mut tx: UartTx<'static, Async>, mut telemetry: Telemetry, cause: ResetCause) {
    let mut frame = [0_u8; MAX_FRAME];
    if let Ok(length) = telemetry.hello(cause, &mut frame) {
        let _ = tx.write(&frame[..length]).await;
    }
    loop {
        let snapshots = SNAPSHOTS.wait().await;
        if NEW_RUN.try_take().is_some() {
            telemetry.next_run();
        }
        for snapshot in snapshots.into_iter().flatten() {
            if let Ok(length) = telemetry.car_snapshot(snapshot, &mut frame) {
                let _ = tx.write(&frame[..length]).await;
            }
        }
    }
}
