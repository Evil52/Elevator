use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub const JSON_SAFE_MAX: u64 = (1 << 53) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageError {
    BadVersion(u8),
    Malformed,
    TrailingBytes,
    OutOfRange,
    TooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetReason {
    PowerOn,
    Button,
    Watchdog,
    Software,
    Brownout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarMode {
    Normal,
    FireRecall,
    Firefighters,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rejection {
    NoSuchFloor,
    DownFromFirst,
    UpFromTop,
    WrongBoot,
    WrongRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandStatus {
    Accepted,
    Applied,
    Rejected(Rejection),
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub device_id: u32,
    pub reset_reason: ResetReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarSnapshot {
    pub car: u8,
    pub mode: CarMode,
    pub doors: DoorState,
    pub floor: Option<u8>,
    pub position_mm: u32,
    pub speed_mm_per_sec: i32,
    pub eta_ms: Option<u32>,
    pub up_calls: u32,
    pub down_calls: u32,
    pub model_time_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFloor {
    pub request_id: u32,
    pub floor: u8,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub request_id: u32,
    pub status: CommandStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Hello(Hello),
    CarSnapshot(CarSnapshot),
    CallFloor(CallFloor),
    CommandResult(CommandResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub version: u8,
    pub boot_id: u32,
    pub run_id: u32,
    pub seq: u32,
    pub message: Message,
}

impl Envelope {
    #[must_use]
    pub fn new(boot_id: u32, run_id: u32, seq: u32, message: Message) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            boot_id,
            run_id,
            seq,
            message,
        }
    }
}

fn validate(envelope: &Envelope) -> Result<(), MessageError> {
    if let Message::CarSnapshot(snapshot) = &envelope.message
        && snapshot.model_time_ms > JSON_SAFE_MAX
    {
        return Err(MessageError::OutOfRange);
    }
    Ok(())
}

pub fn encode_message(envelope: &Envelope, out: &mut [u8]) -> Result<usize, MessageError> {
    validate(envelope)?;
    postcard::to_slice(envelope, out)
        .map(|written| written.len())
        .map_err(|error| match error {
            postcard::Error::SerializeBufferFull => MessageError::TooLong,
            _ => MessageError::Malformed,
        })
}

pub fn decode_message(bytes: &[u8]) -> Result<Envelope, MessageError> {
    let &version = bytes.first().ok_or(MessageError::Malformed)?;
    if version != PROTOCOL_VERSION {
        return Err(MessageError::BadVersion(version));
    }
    let (envelope, rest) =
        postcard::take_from_bytes::<Envelope>(bytes).map_err(|_| MessageError::Malformed)?;
    if !rest.is_empty() {
        return Err(MessageError::TrailingBytes);
    }
    validate(&envelope)?;
    Ok(envelope)
}
