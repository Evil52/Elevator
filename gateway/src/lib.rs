use elevator_wire::protocol::{Envelope, Message, MessageError, decode_message, encode_message};
use elevator_wire::{Decoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub frames: u64,
    pub bad_frames: u64,
    pub bad_messages: u64,
    pub lost_frames: u64,
}

#[derive(Debug)]
pub enum CommandError {
    Json(serde_json::Error),
    NotACommand,
    Message(MessageError),
    Frame,
}

pub struct Bridge {
    decoder: Decoder,
    expected_frame_seq: Option<u8>,
    tx_frame_seq: u8,
    stats: Stats,
    identity: Option<(u32, u32)>,
}

impl Bridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            expected_frame_seq: None,
            tx_frame_seq: 0,
            stats: Stats::default(),
            identity: None,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Envelope> {
        let mut payload = [0_u8; MAX_PAYLOAD];
        let mut envelopes = Vec::new();
        for &byte in bytes {
            match self.decoder.push(byte, &mut payload) {
                None => {}
                Some(Err(_)) => self.stats.bad_frames += 1,
                Some(Ok((frame_seq, length))) => {
                    self.stats.frames += 1;
                    if let Some(expected) = self.expected_frame_seq {
                        self.stats.lost_frames += u64::from(frame_seq.wrapping_sub(expected));
                    }
                    self.expected_frame_seq = Some(frame_seq.wrapping_add(1));
                    match decode_message(&payload[..length]) {
                        Ok(envelope) => {
                            self.identity = Some((envelope.boot_id, envelope.run_id));
                            envelopes.push(envelope);
                        }
                        Err(_) => self.stats.bad_messages += 1,
                    }
                }
            }
        }
        envelopes
    }

    #[must_use]
    pub const fn stats(&self) -> Stats {
        self.stats
    }

    #[must_use]
    pub const fn identity(&self) -> Option<(u32, u32)> {
        self.identity
    }

    pub fn command_frame(&mut self, json: &str) -> Result<Vec<u8>, CommandError> {
        let envelope: Envelope = serde_json::from_str(json).map_err(CommandError::Json)?;
        if !matches!(
            envelope.message,
            Message::CallFloor(_)
                | Message::AckAlarm(_)
                | Message::InjectFault(_)
                | Message::RecoverCar(_)
                | Message::SimControl(_)
                | Message::ReadLog(_)
        ) {
            return Err(CommandError::NotACommand);
        }
        let mut payload = [0_u8; MAX_PAYLOAD];
        let length = encode_message(&envelope, &mut payload).map_err(CommandError::Message)?;
        let mut frame = [0_u8; MAX_FRAME];
        let written = encode_frame(self.tx_frame_seq, &payload[..length], &mut frame)
            .map_err(|_| CommandError::Frame)?;
        self.tx_frame_seq = self.tx_frame_seq.wrapping_add(1);
        Ok(frame[..written].to_vec())
    }
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

pub fn to_json(envelope: &Envelope) -> Result<String, serde_json::Error> {
    serde_json::to_string(envelope)
}
