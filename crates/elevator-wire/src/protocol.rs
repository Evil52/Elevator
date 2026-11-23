use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub const JSON_SAFE_MAX: u64 = (1 << 53) - 1;
pub const ALARMS_PER_PAGE: usize = 8;
pub const EVENTS_PER_CHUNK: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageError {
    BadVersion(u8),
    Malformed,
    TrailingBytes,
    OutOfRange,
    InconsistentPage,
    InconsistentLog,
    TooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ResetReason {
    PowerOn,
    Button,
    Watchdog,
    Software,
    Brownout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CarMode {
    Normal,
    FireRecall,
    Firefighters,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Rejection {
    NoSuchFloor,
    DownFromFirst,
    UpFromTop,
    WrongBoot,
    WrongRun,
    NoSuchAlarm,
    NoSuchCar,
    WrongState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CommandStatus {
    Accepted,
    Applied,
    Rejected(Rejection),
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub device_id: u32,
    pub reset_reason: ResetReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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
    #[cfg_attr(feature = "schema", schemars(range(max = JSON_SAFE_MAX)))]
    pub model_time_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CallFloor {
    pub request_id: u32,
    pub floor: u8,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CommandResult {
    pub request_id: u32,
    pub status: CommandStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AlarmKind {
    FireRecall,
    CarFault,
    LinkLost,
    LogOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AlarmState {
    Active,
    Acknowledged,
    Cleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Alarm {
    pub id: u32,
    pub kind: AlarmKind,
    pub car: Option<u8>,
    pub state: AlarmState,
    #[cfg_attr(feature = "schema", schemars(range(max = JSON_SAFE_MAX)))]
    pub raised_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AlarmPage {
    pub total: u16,
    pub offset: u16,
    pub items: [Option<Alarm>; ALARMS_PER_PAGE],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AckAlarm {
    pub request_id: u32,
    pub alarm_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct InjectFault {
    pub request_id: u32,
    pub car: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RecoverCar {
    pub request_id: u32,
    pub car: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SimAction {
    Pause,
    Resume,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SimControl {
    pub request_id: u32,
    pub action: SimAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Assigned,
    Arrived,
    Served,
    Requeued,
    Cancelled,
    AlarmRaised,
    AlarmAcked,
    AlarmCleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub index: u32,
    #[cfg_attr(feature = "schema", schemars(range(max = JSON_SAFE_MAX)))]
    pub at_ms: u64,
    pub kind: EventKind,
    pub car: Option<u8>,
    pub id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct EventLogChunk {
    pub first_index: u32,
    pub dropped_before: u32,
    pub events: [Option<Event>; EVENTS_PER_CHUNK],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReadLog {
    pub request_id: u32,
    pub from_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub counter: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Message {
    Hello(Hello),
    CarSnapshot(CarSnapshot),
    CallFloor(CallFloor),
    CommandResult(CommandResult),
    AlarmPage(AlarmPage),
    AckAlarm(AckAlarm),
    InjectFault(InjectFault),
    RecoverCar(RecoverCar),
    SimControl(SimControl),
    EventLogChunk(EventLogChunk),
    ReadLog(ReadLog),
    Ping(Ping),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    #[cfg_attr(feature = "schema", schemars(extend("const" = PROTOCOL_VERSION)))]
    pub version: u8,
    pub boot_id: u32,
    pub run_id: u32,
    pub seq: u32,
    pub message: Message,
}

impl AlarmPage {
    fn validate(&self) -> Result<(), MessageError> {
        let count = self.items.iter().take_while(|item| item.is_some()).count();
        let remaining = self
            .total
            .checked_sub(self.offset)
            .ok_or(MessageError::InconsistentPage)?;
        if self.items[count..].iter().any(Option::is_some)
            || count > usize::from(remaining)
            || (count == 0 && remaining != 0)
        {
            return Err(MessageError::InconsistentPage);
        }
        if self
            .items
            .iter()
            .flatten()
            .any(|alarm| alarm.raised_at_ms > JSON_SAFE_MAX)
        {
            return Err(MessageError::OutOfRange);
        }
        Ok(())
    }
}

impl EventLogChunk {
    fn validate(&self) -> Result<(), MessageError> {
        let mut expected = Some(self.first_index);
        for item in &self.events {
            if let Some(event) = item {
                if Some(event.index) != expected {
                    return Err(MessageError::InconsistentLog);
                }
                if event.at_ms > JSON_SAFE_MAX {
                    return Err(MessageError::OutOfRange);
                }
                expected = event.index.checked_add(1);
            } else {
                expected = None;
            }
        }
        Ok(())
    }
}

impl Envelope {
    pub fn validate(&self) -> Result<(), MessageError> {
        if self.version != PROTOCOL_VERSION {
            return Err(MessageError::BadVersion(self.version));
        }
        match &self.message {
            Message::CarSnapshot(snapshot) if snapshot.model_time_ms > JSON_SAFE_MAX => {
                Err(MessageError::OutOfRange)
            }
            Message::AlarmPage(page) => page.validate(),
            Message::EventLogChunk(chunk) => chunk.validate(),
            _ => Ok(()),
        }
    }

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

pub fn encode_message(envelope: &Envelope, out: &mut [u8]) -> Result<usize, MessageError> {
    envelope.validate()?;
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
    envelope.validate()?;
    Ok(envelope)
}
