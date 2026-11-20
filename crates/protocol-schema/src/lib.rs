use elevator_wire::protocol::{
    ALARMS_PER_PAGE, AckAlarm, Alarm, AlarmKind, AlarmPage, AlarmState, CallFloor, CarMode,
    CarSnapshot, CommandResult, CommandStatus, Direction, DoorState, EVENTS_PER_CHUNK, Envelope,
    Event, EventKind, EventLogChunk, Hello, InjectFault, JSON_SAFE_MAX, Message, ReadLog,
    RecoverCar, Rejection, ResetReason, SimAction, SimControl,
};
use schemars::{
    Schema,
    transform::{RecursiveTransform, Transform},
};
use serde_json::{Value, json};
use std::sync::LazyLock;

pub const SEMANTIC_ONLY: &[&str] = &[
    "alarm_page_total",
    "alarm_page_offset",
    "alarm_page_gap",
    "alarm_page_no_progress",
    "log_first_index",
    "log_index_gap",
    "log_empty_slot",
    "log_index_overflow",
];

fn envelope_schema() -> Schema {
    let mut schema = schemars::schema_for!(Envelope);
    RecursiveTransform(
        |node: &mut Schema| match node.get("format").and_then(Value::as_str) {
            Some("uint32") => {
                node.insert("maximum".into(), u32::MAX.into());
            }
            Some("int32") => {
                node.insert("minimum".into(), i32::MIN.into());
                node.insert("maximum".into(), i32::MAX.into());
            }
            _ => {}
        },
    )
    .transform(&mut schema);
    schema
}

#[must_use]
pub fn schema_json() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&envelope_schema()).unwrap_or_default()
    )
}

static VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    jsonschema::validator_for(envelope_schema().as_value()).map_err(|e| e.to_string())
});

// После проверки схемы все числовые поля — целые в точном диапазоне JSON.
// Serde для целочисленных полей требует одинакового представления 5, 5.0 и 5e0.
#[allow(clippy::cast_possible_truncation)]
fn normalize_integers(value: &mut Value) {
    match value {
        Value::Number(n) if n.is_f64() => {
            if let Some(number) = n.as_f64() {
                *n = (number as i64).into();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_integers),
        Value::Object(fields) => fields.values_mut().for_each(normalize_integers),
        _ => {}
    }
}

pub fn validate_json(value: &Value) -> Result<Envelope, String> {
    VALIDATOR
        .as_ref()
        .map_err(Clone::clone)?
        .validate(value)
        .map_err(|e| e.to_string())?;
    let mut normalized = value.clone();
    normalize_integers(&mut normalized);
    let envelope: Envelope = serde_json::from_value(normalized).map_err(|e| e.to_string())?;
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

fn pagination_examples(alarm: Alarm, event: Event) -> [(&'static str, Message); 3] {
    let mut items = [None; ALARMS_PER_PAGE];
    items[0] = Some(alarm);
    [
        (
            "alarm_page_partial",
            Message::AlarmPage(AlarmPage {
                offset: 2,
                total: 4,
                items,
            }),
        ),
        (
            "alarm_page_at_end",
            Message::AlarmPage(AlarmPage {
                offset: 4,
                total: 4,
                items: [None; ALARMS_PER_PAGE],
            }),
        ),
        (
            "log_at_last_index",
            event_chunk(&[Event {
                index: u32::MAX,
                ..event
            }]),
        ),
    ]
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
        .chain(pagination_examples(alarms[0], events[0]))
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

fn inconsistent_pages() -> [(&'static str, Value); 8] {
    [
        (
            "alarm_page_total",
            mutated("alarm_page", |v| {
                v["message"]["alarmPage"]["total"] = json!(0);
            }),
        ),
        (
            "alarm_page_offset",
            mutated("alarm_page_empty", |v| {
                v["message"]["alarmPage"]["offset"] = json!(1);
            }),
        ),
        (
            "alarm_page_gap",
            mutated("alarm_page", |v| {
                v["message"]["alarmPage"]["items"][0] = Value::Null;
            }),
        ),
        (
            "alarm_page_no_progress",
            mutated("alarm_page_empty", |v| {
                v["message"]["alarmPage"]["total"] = json!(1);
            }),
        ),
        (
            "log_first_index",
            mutated("event_log_chunk", |v| {
                v["message"]["eventLogChunk"]["firstIndex"] = json!(12);
            }),
        ),
        (
            "log_index_gap",
            mutated("event_log_chunk", |v| {
                v["message"]["eventLogChunk"]["events"][1]["index"] = json!(12);
            }),
        ),
        (
            "log_empty_slot",
            mutated("event_log_chunk", |v| {
                v["message"]["eventLogChunk"]["events"][0] = Value::Null;
            }),
        ),
        (
            "log_index_overflow",
            mutated("event_log_chunk", |v| {
                let chunk = &mut v["message"]["eventLogChunk"];
                chunk["firstIndex"] = json!(u32::MAX);
                chunk["events"][0]["index"] = json!(u32::MAX);
                chunk["events"][1]["index"] = json!(0);
            }),
        ),
    ]
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
        (
            "request_id_over_u32",
            call(|v| v["message"]["callFloor"]["requestId"] = json!(u64::from(u32::MAX) + 1)),
        ),
        (
            "speed_below_i32",
            mutated("car_snapshot", |v| {
                v["message"]["carSnapshot"]["speedMmPerSec"] = json!(i64::from(i32::MIN) - 1);
            }),
        ),
        (
            "speed_above_i32",
            mutated("car_snapshot", |v| {
                v["message"]["carSnapshot"]["speedMmPerSec"] = json!(i64::from(i32::MAX) + 1);
            }),
        ),
        (
            "positional_call",
            call(|v| v["message"]["callFloor"] = json!([7, 5, "down"])),
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
        .chain(inconsistent_pages())
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}
