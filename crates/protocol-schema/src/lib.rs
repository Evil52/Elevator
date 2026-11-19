use elevator_wire::protocol::{
    ALARMS_PER_PAGE, AckAlarm, Alarm, AlarmKind, AlarmPage, AlarmState, CallFloor, CarMode,
    CarSnapshot, CommandResult, CommandStatus, Direction, DoorState, EVENTS_PER_CHUNK, Envelope,
    Event, EventKind, EventLogChunk, Hello, InjectFault, JSON_SAFE_MAX, Message, ReadLog,
    RecoverCar, Rejection, ResetReason, SimAction, SimControl,
};
use serde_json::{Value, json};

pub const SEMANTIC_ONLY: [&str; 1] = ["unknown_version"];

#[must_use]
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Envelope);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).unwrap_or_default()
    )
}

pub fn validate_json(value: &Value) -> Result<Envelope, String> {
    let envelope: Envelope = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    envelope.validate().map_err(|e| format!("{e:?}"))?;
    Ok(envelope)
}

fn envelope(message: Message) -> Envelope {
    Envelope::new(1, 2, 3, message)
}

fn snapshot(model_time_ms: u64) -> Message {
    Message::CarSnapshot(CarSnapshot {
        car: 1,
        mode: CarMode::Normal,
        doors: DoorState::Open,
        floor: Some(3),
        position_mm: 6000,
        speed_mm_per_sec: -250,
        eta_ms: Some(4200),
        up_calls: 0b101,
        down_calls: 0b10,
        model_time_ms,
    })
}

fn alarm_page(alarms: &[Alarm]) -> Message {
    let mut items = [None; ALARMS_PER_PAGE];
    for (slot, alarm) in items.iter_mut().zip(alarms) {
        *slot = Some(*alarm);
    }
    Message::AlarmPage(AlarmPage {
        total: u16::try_from(alarms.len()).unwrap_or(u16::MAX),
        offset: 0,
        items,
    })
}

fn event_chunk(events: &[Event]) -> Message {
    let mut items = [None; EVENTS_PER_CHUNK];
    for (slot, event) in items.iter_mut().zip(events) {
        *slot = Some(*event);
    }
    Message::EventLogChunk(EventLogChunk {
        first_index: events.first().map_or(0, |event| event.index),
        dropped_before: 0,
        events: items,
    })
}

fn call_floor() -> Message {
    Message::CallFloor(CallFloor {
        request_id: 7,
        floor: 5,
        direction: Direction::Down,
    })
}

fn to_value(envelope: &Envelope) -> Value {
    serde_json::to_value(envelope).unwrap_or(Value::Null)
}

#[must_use]
pub fn valid_fixtures() -> Vec<(String, Value)> {
    let alarms = [
        Alarm {
            id: 1,
            kind: AlarmKind::FireRecall,
            car: None,
            state: AlarmState::Active,
            raised_at_ms: 1000,
        },
        Alarm {
            id: 2,
            kind: AlarmKind::CarFault,
            car: Some(1),
            state: AlarmState::Acknowledged,
            raised_at_ms: 5,
        },
    ];
    let events = [
        Event {
            index: 10,
            at_ms: 1000,
            kind: EventKind::Assigned,
            car: Some(2),
            id: 7,
        },
        Event {
            index: 11,
            at_ms: 2000,
            kind: EventKind::Served,
            car: None,
            id: 7,
        },
    ];
    let cases = [
        (
            "hello",
            Message::Hello(Hello {
                device_id: 0xDEAD_BEEF,
                reset_reason: ResetReason::Watchdog,
            }),
        ),
        ("car_snapshot", snapshot(1000)),
        ("car_snapshot_at_json_limit", snapshot(JSON_SAFE_MAX)),
        ("call_floor", call_floor()),
        (
            "command_result",
            Message::CommandResult(CommandResult {
                request_id: 300,
                status: CommandStatus::Rejected(Rejection::UpFromTop),
            }),
        ),
        ("alarm_page", alarm_page(&alarms)),
        ("alarm_page_empty", alarm_page(&[])),
        (
            "ack_alarm",
            Message::AckAlarm(AckAlarm {
                request_id: 9,
                alarm_id: 300,
            }),
        ),
        (
            "inject_fault",
            Message::InjectFault(InjectFault {
                request_id: 10,
                car: 0,
            }),
        ),
        (
            "recover_car",
            Message::RecoverCar(RecoverCar {
                request_id: 11,
                car: 0,
            }),
        ),
        (
            "sim_control",
            Message::SimControl(SimControl {
                request_id: 12,
                action: SimAction::Reset,
            }),
        ),
        ("event_log_chunk", event_chunk(&events)),
        (
            "read_log",
            Message::ReadLog(ReadLog {
                request_id: 13,
                from_index: 300,
            }),
        ),
    ];
    cases
        .into_iter()
        .map(|(name, message)| (name.to_owned(), to_value(&envelope(message))))
        .collect()
}

fn base(name: &str) -> Value {
    valid_fixtures()
        .into_iter()
        .find(|(candidate, _)| candidate == name)
        .map_or(Value::Null, |(_, value)| value)
}

fn mutated(name: &str, change: impl FnOnce(&mut Value)) -> Value {
    let mut value = base(name);
    change(&mut value);
    value
}

#[must_use]
pub fn invalid_fixtures() -> Vec<(String, Value)> {
    let call = |change: fn(&mut Value)| mutated("call_floor", change);
    let cases = [
        (
            "time_over_json_limit",
            mutated("car_snapshot", |v| {
                v["message"]["carSnapshot"]["modelTimeMs"] = json!(9_007_199_254_740_992_u64);
            }),
        ),
        (
            "negative_floor",
            call(|v| v["message"]["callFloor"]["floor"] = json!(-1)),
        ),
        (
            "floor_over_u8",
            call(|v| v["message"]["callFloor"]["floor"] = json!(256)),
        ),
        ("unknown_version", call(|v| v["version"] = json!(2))),
        (
            "unknown_message_kind",
            call(|v| v["message"] = json!({ "bogus": {} })),
        ),
        (
            "unknown_enum_value",
            call(|v| v["message"]["callFloor"]["direction"] = json!("sideways")),
        ),
        (
            "missing_field",
            call(|v| {
                if let Some(fields) = v["message"]["callFloor"].as_object_mut() {
                    fields.remove("requestId");
                }
            }),
        ),
        (
            "extra_field",
            call(|v| v["message"]["callFloor"]["extra"] = json!(1)),
        ),
        (
            "fractional_position",
            mutated("car_snapshot", |v| {
                v["message"]["carSnapshot"]["positionMm"] = json!(1.5);
            }),
        ),
        (
            "string_instead_of_number",
            call(|v| v["message"]["callFloor"]["requestId"] = json!("7")),
        ),
        (
            "page_with_too_many_items",
            mutated("alarm_page", |v| {
                if let Some(items) = v["message"]["alarmPage"]["items"].as_array_mut() {
                    items.push(Value::Null);
                }
            }),
        ),
        (
            "page_with_too_few_items",
            mutated("alarm_page", |v| {
                if let Some(items) = v["message"]["alarmPage"]["items"].as_array_mut() {
                    items.pop();
                }
            }),
        ),
        ("null_envelope", Value::Null),
        (
            "two_messages_at_once",
            call(|v| v["message"]["hello"] = json!({ "deviceId": 1, "resetReason": "powerOn" })),
        ),
    ];
    cases
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}
