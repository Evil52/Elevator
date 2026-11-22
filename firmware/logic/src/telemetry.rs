use elevator_wire::protocol::{
    CarSnapshot, Envelope, Hello, Message, PROTOCOL_VERSION, ResetReason, encode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, encode_frame};

use crate::reset::ResetCause;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    Message,
    Frame,
}

#[must_use]
pub const fn reset_reason(cause: ResetCause) -> ResetReason {
    match cause {
        ResetCause::IndependentWatchdog | ResetCause::WindowWatchdog => ResetReason::Watchdog,
        ResetCause::PowerOn => ResetReason::PowerOn,
        ResetCause::BrownOut => ResetReason::Brownout,
        ResetCause::Pin => ResetReason::Button,
        ResetCause::Software | ResetCause::LowPower | ResetCause::Unknown => ResetReason::Software,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Telemetry {
    device_id: u32,
    boot_id: u32,
    run_id: u32,
    seq: u32,
    frame_seq: u8,
}

impl Telemetry {
    #[must_use]
    pub const fn new(device_id: u32, boot_id: u32) -> Self {
        Self {
            device_id,
            boot_id,
            run_id: 1,
            seq: 0,
            frame_seq: 0,
        }
    }

    pub const fn next_run(&mut self) {
        self.run_id = self.run_id.wrapping_add(1);
    }

    pub fn hello(
        &mut self,
        cause: ResetCause,
        out: &mut [u8; MAX_FRAME],
    ) -> Result<usize, LinkError> {
        self.emit(
            Message::Hello(Hello {
                device_id: self.device_id,
                reset_reason: reset_reason(cause),
            }),
            out,
        )
    }

    pub fn car_snapshot(
        &mut self,
        snapshot: CarSnapshot,
        out: &mut [u8; MAX_FRAME],
    ) -> Result<usize, LinkError> {
        self.emit(Message::CarSnapshot(snapshot), out)
    }

    fn emit(&mut self, message: Message, out: &mut [u8; MAX_FRAME]) -> Result<usize, LinkError> {
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            boot_id: self.boot_id,
            run_id: self.run_id,
            seq: self.seq,
            message,
        };
        let mut payload = [0_u8; MAX_PAYLOAD];
        let length = encode_message(&envelope, &mut payload).map_err(|_| LinkError::Message)?;
        let written =
            encode_frame(self.frame_seq, &payload[..length], out).map_err(|_| LinkError::Frame)?;
        self.seq = self.seq.wrapping_add(1);
        self.frame_seq = self.frame_seq.wrapping_add(1);
        Ok(written)
    }
}
