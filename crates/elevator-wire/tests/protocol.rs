use elevator_wire::protocol::{
    ALARMS_PER_PAGE, AckAlarm, Alarm, AlarmKind, AlarmPage, AlarmState, CallFloor, CarMode,
    CarSnapshot, CommandResult, CommandStatus, Direction, DoorState, EVENTS_PER_CHUNK, Envelope,
    Event, EventKind, EventLogChunk, Hello, InjectFault, JSON_SAFE_MAX, Message, MessageError,
    Ping, ReadLog, RecoverCar, Rejection, ResetReason, SimAction, SimControl, decode_message,
    encode_message,
};
use elevator_wire::{Decoder, MAX_FRAME, MAX_PAYLOAD, decode_frame, encode_frame};

fn call_floor() -> Envelope {
    Envelope::new(
        1,
        2,
        3,
        Message::CallFloor(CallFloor {
            request_id: 7,
            floor: 5,
            direction: Direction::Down,
        }),
    )
}

fn command_result() -> Envelope {
    Envelope::new(
        0,
        0,
        0,
        Message::CommandResult(CommandResult {
            request_id: 300,
            status: CommandStatus::Rejected(Rejection::UpFromTop),
        }),
    )
}

fn snapshot() -> Envelope {
    Envelope::new(
        0,
        0,
        0,
        Message::CarSnapshot(CarSnapshot {
            car: 1,
            mode: CarMode::FireRecall,
            doors: DoorState::Open,
            floor: Some(3),
            position_mm: 6000,
            speed_mm_per_sec: -1,
            eta_ms: None,
            up_calls: 0b101,
            down_calls: 0,
            model_time_ms: 1000,
        }),
    )
}

fn hello() -> Envelope {
    Envelope::new(
        10,
        20,
        30,
        Message::Hello(Hello {
            device_id: 0xDEAD_BEEF,
            reset_reason: ResetReason::Watchdog,
        }),
    )
}

fn snapshot_with_time(model_time_ms: u64) -> Envelope {
    let mut envelope = snapshot();
    if let Message::CarSnapshot(snapshot) = &mut envelope.message {
        snapshot.model_time_ms = model_time_ms;
    }
    envelope
}

fn encoded(envelope: &Envelope) -> Vec<u8> {
    let mut buf = [0u8; MAX_PAYLOAD];
    let n = encode_message(envelope, &mut buf).unwrap();
    buf[..n].to_vec()
}

fn largest_messages() -> Vec<Envelope> {
    let ids = (u32::MAX, u32::MAX, u32::MAX);
    vec![
        Envelope::new(
            ids.0,
            ids.1,
            ids.2,
            Message::Hello(Hello {
                device_id: u32::MAX,
                reset_reason: ResetReason::Brownout,
            }),
        ),
        Envelope::new(
            ids.0,
            ids.1,
            ids.2,
            Message::CarSnapshot(CarSnapshot {
                car: u8::MAX,
                mode: CarMode::Unavailable,
                doors: DoorState::Closing,
                floor: Some(u8::MAX),
                position_mm: u32::MAX,
                speed_mm_per_sec: i32::MIN,
                eta_ms: Some(u32::MAX),
                up_calls: u32::MAX,
                down_calls: u32::MAX,
                model_time_ms: JSON_SAFE_MAX,
            }),
        ),
        Envelope::new(
            ids.0,
            ids.1,
            ids.2,
            Message::CallFloor(CallFloor {
                request_id: u32::MAX,
                floor: u8::MAX,
                direction: Direction::Down,
            }),
        ),
        Envelope::new(
            ids.0,
            ids.1,
            ids.2,
            Message::CommandResult(CommandResult {
                request_id: u32::MAX,
                status: CommandStatus::Rejected(Rejection::WrongRun),
            }),
        ),
    ]
}

#[test]
fn every_message_kind_round_trips() {
    for envelope in [call_floor(), command_result(), snapshot(), hello()] {
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn the_largest_values_round_trip_and_fit_one_frame() {
    for envelope in largest_messages() {
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert!(bytes.len() <= MAX_PAYLOAD, "{} bytes", bytes.len());
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn call_floor_has_known_bytes() {
    let expected = [0x01, 0x01, 0x02, 0x03, 0x02, 0x07, 0x05, 0x01];
    assert_eq!(encoded(&call_floor()), expected);
    assert_eq!(decode_message(&expected), Ok(call_floor()));
}

#[test]
fn command_result_has_known_bytes() {
    let expected = [0x01, 0x00, 0x00, 0x00, 0x03, 0xAC, 0x02, 0x02, 0x02];
    assert_eq!(encoded(&command_result()), expected);
    assert_eq!(decode_message(&expected), Ok(command_result()));
}

#[test]
fn car_snapshot_has_known_bytes() {
    let expected = [
        0x01, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03, 0xF0, 0x2E, 0x01, 0x00, 0x05,
        0x00, 0xE8, 0x07,
    ];
    assert_eq!(encoded(&snapshot()), expected);
    assert_eq!(decode_message(&expected), Ok(snapshot()));
}

#[test]
fn an_unknown_version_is_reported_before_anything_else() {
    assert_eq!(
        decode_message(&[0x02, 0xFF, 0xFF]),
        Err(MessageError::BadVersion(2))
    );
    assert_eq!(decode_message(&[0x00]), Err(MessageError::BadVersion(0)));
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = encoded(&call_floor());
    assert_ne!(bytes.len(), 0, "nothing encoded");
    bytes.push(0x00);
    assert_eq!(decode_message(&bytes), Err(MessageError::TrailingBytes));
}

#[test]
fn every_prefix_of_a_message_is_malformed() {
    for envelope in [call_floor(), command_result(), snapshot(), hello()] {
        let bytes = encoded(&envelope);
        assert!(bytes.len() > 4, "message too short: {bytes:02X?}");
        for cut in 0..bytes.len() {
            assert_eq!(
                decode_message(&bytes[..cut]),
                Err(MessageError::Malformed),
                "prefix of {cut} bytes of {envelope:?}"
            );
        }
    }
}

#[test]
fn an_unknown_message_kind_is_malformed() {
    assert_eq!(
        decode_message(&[0x01, 0x00, 0x00, 0x00, 0x09]),
        Err(MessageError::Malformed)
    );
}

#[test]
fn an_unknown_enum_value_is_malformed() {
    assert_eq!(
        decode_message(&[0x01, 0x00, 0x00, 0x00, 0x02, 0x07, 0x05, 0x05]),
        Err(MessageError::Malformed)
    );
}

#[test]
fn a_time_at_the_json_limit_round_trips() {
    let envelope = snapshot_with_time(JSON_SAFE_MAX);
    let bytes = encoded(&envelope);
    assert_ne!(bytes.len(), 0, "nothing encoded");
    assert_eq!(decode_message(&bytes), Ok(envelope));
}

#[test]
fn a_time_over_the_json_limit_is_rejected_on_encode() {
    let mut buf = [0u8; MAX_PAYLOAD];
    assert_eq!(
        encode_message(&snapshot_with_time(JSON_SAFE_MAX + 1), &mut buf),
        Err(MessageError::OutOfRange)
    );
}

#[test]
fn a_time_over_the_json_limit_is_rejected_on_decode() {
    let mut buf = [0u8; MAX_PAYLOAD];
    let raw = postcard::to_slice(&snapshot_with_time(JSON_SAFE_MAX + 1), &mut buf).unwrap();
    assert_eq!(decode_message(raw), Err(MessageError::OutOfRange));
}

#[test]
fn encode_reports_too_long_for_a_small_buffer() {
    let mut buf = [0u8; 3];
    assert_eq!(
        encode_message(&call_floor(), &mut buf),
        Err(MessageError::TooLong)
    );
}

#[test]
fn a_message_travels_through_a_frame() {
    let envelope = snapshot();
    let bytes = encoded(&envelope);
    assert_ne!(bytes.len(), 0, "nothing encoded");

    let mut wire = [0u8; MAX_FRAME];
    let n = encode_frame(9, &bytes, &mut wire).unwrap();

    let mut decoder = Decoder::new();
    let mut payload = [0u8; MAX_PAYLOAD];
    let mut found = None;
    for &byte in &wire[..n] {
        if let Some(result) = decoder.push(byte, &mut payload) {
            found = Some(result);
        }
    }
    let (seq, len) = found.expect("no frame").unwrap();
    assert_eq!(seq, 9);
    assert_eq!(decode_message(&payload[..len]), Ok(envelope));
}

#[test]
fn a_frame_body_decodes_without_the_streaming_decoder() {
    let envelope = hello();
    let bytes = encoded(&envelope);
    assert_ne!(bytes.len(), 0, "nothing encoded");

    let mut wire = [0u8; MAX_FRAME];
    let n = encode_frame(1, &bytes, &mut wire).unwrap();
    let mut payload = [0u8; MAX_PAYLOAD];
    let (_, len) = decode_frame(&wire[..n - 1], &mut payload).unwrap();
    assert_eq!(decode_message(&payload[..len]), Ok(envelope));
}

fn alarm(id: u32, kind: AlarmKind, car: Option<u8>, state: AlarmState, raised_at_ms: u64) -> Alarm {
    Alarm {
        id,
        kind,
        car,
        state,
        raised_at_ms,
    }
}

fn page_of(total: u16, offset: u16, alarms: &[Alarm]) -> Envelope {
    let mut items = [None; ALARMS_PER_PAGE];
    for (slot, alarm) in items.iter_mut().zip(alarms) {
        *slot = Some(*alarm);
    }
    Envelope::new(
        0,
        0,
        0,
        Message::AlarmPage(AlarmPage {
            total,
            offset,
            items,
        }),
    )
}

fn two_alarm_page() -> Envelope {
    page_of(
        2,
        0,
        &[
            alarm(1, AlarmKind::FireRecall, None, AlarmState::Active, 1000),
            alarm(2, AlarmKind::CarFault, Some(1), AlarmState::Acknowledged, 5),
        ],
    )
}

fn ack_alarm() -> Envelope {
    Envelope::new(
        1,
        2,
        3,
        Message::AckAlarm(AckAlarm {
            request_id: 9,
            alarm_id: 300,
        }),
    )
}

fn commands() -> Vec<Envelope> {
    vec![
        ack_alarm(),
        Envelope::new(
            4,
            5,
            6,
            Message::InjectFault(InjectFault {
                request_id: u32::MAX,
                car: 0,
            }),
        ),
        Envelope::new(
            4,
            5,
            6,
            Message::RecoverCar(RecoverCar {
                request_id: 1,
                car: u8::MAX,
            }),
        ),
        Envelope::new(
            4,
            5,
            6,
            Message::SimControl(SimControl {
                request_id: 2,
                action: SimAction::Reset,
            }),
        ),
    ]
}

fn full_page_at_the_limits() -> Envelope {
    let alarms = [alarm(
        u32::MAX,
        AlarmKind::LogOverflow,
        Some(u8::MAX),
        AlarmState::Cleared,
        JSON_SAFE_MAX,
    ); ALARMS_PER_PAGE];
    page_of(
        u16::MAX,
        u16::MAX - u16::try_from(alarms.len()).unwrap(),
        &alarms,
    )
}

fn page_with_time_in_slot(slot: usize, raised_at_ms: u64) -> Envelope {
    let mut envelope = full_page_at_the_limits();
    if let Message::AlarmPage(page) = &mut envelope.message {
        page.items[slot] = Some(alarm(
            9,
            AlarmKind::LinkLost,
            None,
            AlarmState::Active,
            raised_at_ms,
        ));
    }
    envelope
}

#[test]
fn alarm_pages_round_trip() {
    let empty = page_of(0, 0, &[]);
    let full = page_of(
        u16::try_from(ALARMS_PER_PAGE).unwrap(),
        0,
        &[alarm(7, AlarmKind::LinkLost, Some(3), AlarmState::Cleared, 42); ALARMS_PER_PAGE],
    );
    for envelope in [empty, two_alarm_page(), full, full_page_at_the_limits()] {
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn a_full_alarm_page_at_the_limits_fits_one_frame() {
    let bytes = encoded(&full_page_at_the_limits());
    assert!(bytes.len() <= MAX_PAYLOAD, "{} bytes", bytes.len());
}

#[test]
fn the_new_commands_round_trip() {
    for envelope in commands() {
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn ack_alarm_has_known_bytes() {
    let expected = [0x01, 0x01, 0x02, 0x03, 0x05, 0x09, 0xAC, 0x02];
    assert_eq!(encoded(&ack_alarm()), expected);
    assert_eq!(decode_message(&expected), Ok(ack_alarm()));
}

#[test]
fn alarm_page_has_known_bytes() {
    let expected = [
        0x01, 0x00, 0x00, 0x00, 0x04, 0x02, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0xE8, 0x07, 0x01,
        0x02, 0x01, 0x01, 0x01, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(encoded(&two_alarm_page()), expected);
    assert_eq!(decode_message(&expected), Ok(two_alarm_page()));
}

#[test]
fn an_unknown_alarm_kind_is_malformed() {
    let mut bytes = encoded(&two_alarm_page());
    assert_ne!(bytes.len(), 0, "nothing encoded");
    bytes[9] = 0x09;
    assert_eq!(decode_message(&bytes), Err(MessageError::Malformed));
}

#[test]
fn an_unknown_sim_action_is_malformed() {
    assert_eq!(
        decode_message(&[0x01, 0x00, 0x00, 0x00, 0x08, 0x02, 0x07]),
        Err(MessageError::Malformed)
    );
}

#[test]
fn the_new_rejections_round_trip() {
    for rejection in [
        Rejection::NoSuchAlarm,
        Rejection::NoSuchCar,
        Rejection::WrongState,
    ] {
        let envelope = Envelope::new(
            0,
            0,
            0,
            Message::CommandResult(CommandResult {
                request_id: 1,
                status: CommandStatus::Rejected(rejection),
            }),
        );
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn an_alarm_time_at_the_json_limit_round_trips() {
    let envelope = page_with_time_in_slot(7, JSON_SAFE_MAX);
    let bytes = encoded(&envelope);
    assert_ne!(bytes.len(), 0, "nothing encoded");
    assert_eq!(decode_message(&bytes), Ok(envelope));
}

#[test]
fn an_alarm_time_over_the_json_limit_is_rejected_in_any_slot() {
    for slot in 0..ALARMS_PER_PAGE {
        let envelope = page_with_time_in_slot(slot, JSON_SAFE_MAX + 1);
        let mut buf = [0u8; MAX_PAYLOAD];
        assert_eq!(
            encode_message(&envelope, &mut buf),
            Err(MessageError::OutOfRange),
            "encode, slot {slot}"
        );
        let raw = postcard::to_slice(&envelope, &mut buf).unwrap();
        assert_eq!(
            decode_message(raw),
            Err(MessageError::OutOfRange),
            "decode, slot {slot}"
        );
    }
}

fn event(index: u32, at_ms: u64, kind: EventKind, car: Option<u8>, id: u32) -> Event {
    Event {
        index,
        at_ms,
        kind,
        car,
        id,
    }
}

fn chunk_of(first_index: u32, dropped_before: u32, events: &[Event]) -> Envelope {
    let mut items = [None; EVENTS_PER_CHUNK];
    for (slot, event) in items.iter_mut().zip(events) {
        *slot = Some(*event);
    }
    Envelope::new(
        0,
        0,
        0,
        Message::EventLogChunk(EventLogChunk {
            first_index,
            dropped_before,
            events: items,
        }),
    )
}

fn two_event_chunk() -> Envelope {
    chunk_of(
        10,
        0,
        &[
            event(10, 1000, EventKind::Assigned, Some(2), 7),
            event(11, 2000, EventKind::Served, None, 7),
        ],
    )
}

fn read_log() -> Envelope {
    Envelope::new(
        1,
        2,
        3,
        Message::ReadLog(ReadLog {
            request_id: 5,
            from_index: 300,
        }),
    )
}

fn ping() -> Envelope {
    Envelope::new(1, 2, 3, Message::Ping(Ping { counter: 5 }))
}

fn full_chunk_at_the_limits() -> Envelope {
    let first = u32::MAX - u32::try_from(EVENTS_PER_CHUNK - 1).unwrap();
    let events: [_; EVENTS_PER_CHUNK] = core::array::from_fn(|i| {
        event(
            first + u32::try_from(i).unwrap(),
            JSON_SAFE_MAX,
            EventKind::AlarmCleared,
            Some(u8::MAX),
            u32::MAX,
        )
    });
    chunk_of(first, first, &events)
}

fn chunk_with_time_in_slot(slot: usize, at_ms: u64) -> Envelope {
    let mut envelope = full_chunk_at_the_limits();
    if let Message::EventLogChunk(chunk) = &mut envelope.message {
        chunk.events[slot].as_mut().unwrap().at_ms = at_ms;
    }
    envelope
}

#[test]
fn event_log_chunks_round_trip() {
    let kinds = [
        EventKind::Assigned,
        EventKind::Arrived,
        EventKind::Served,
        EventKind::Requeued,
        EventKind::Cancelled,
        EventKind::AlarmRaised,
        EventKind::AlarmAcked,
        EventKind::AlarmCleared,
    ];
    let all_kinds: [_; EVENTS_PER_CHUNK] =
        core::array::from_fn(|i| event(5 + u32::try_from(i).unwrap(), 6, kinds[i], Some(1), 2));
    for envelope in [
        chunk_of(0, 0, &[]),
        two_event_chunk(),
        chunk_of(5, 3, &all_kinds),
        full_chunk_at_the_limits(),
        read_log(),
        ping(),
    ] {
        let bytes = encoded(&envelope);
        assert_ne!(bytes.len(), 0, "nothing encoded for {envelope:?}");
        assert_eq!(decode_message(&bytes), Ok(envelope));
    }
}

#[test]
fn a_full_event_chunk_at_the_limits_fits_one_frame() {
    let bytes = encoded(&full_chunk_at_the_limits());
    assert_ne!(bytes.len(), 0, "nothing encoded");
    assert!(bytes.len() <= MAX_PAYLOAD, "{} bytes", bytes.len());
}

#[test]
fn read_log_has_known_bytes() {
    let expected = [0x01, 0x01, 0x02, 0x03, 0x0A, 0x05, 0xAC, 0x02];
    assert_eq!(encoded(&read_log()), expected);
    assert_eq!(decode_message(&expected), Ok(read_log()));
}

#[test]
fn ping_has_known_bytes() {
    let expected = [0x01, 0x01, 0x02, 0x03, 0x0B, 0x05];
    assert_eq!(encoded(&ping()), expected);
    assert_eq!(decode_message(&expected), Ok(ping()));
}

#[test]
fn ping_counter_uses_the_full_u32_range() {
    let envelope = Envelope::new(1, 2, 3, Message::Ping(Ping { counter: u32::MAX }));
    assert_eq!(decode_message(&encoded(&envelope)), Ok(envelope));
}

#[test]
fn event_log_chunk_has_known_bytes() {
    let expected = [
        0x01, 0x00, 0x00, 0x00, 0x09, 0x0A, 0x00, 0x01, 0x0A, 0xE8, 0x07, 0x00, 0x01, 0x02, 0x07,
        0x01, 0x0B, 0xD0, 0x0F, 0x02, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(encoded(&two_event_chunk()), expected);
    assert_eq!(decode_message(&expected), Ok(two_event_chunk()));
}

#[test]
fn an_unknown_event_kind_is_malformed() {
    let mut bytes = encoded(&two_event_chunk());
    assert_ne!(bytes.len(), 0, "nothing encoded");
    bytes[11] = 0x09;
    assert_eq!(decode_message(&bytes), Err(MessageError::Malformed));
}

#[test]
fn an_event_time_at_the_json_limit_round_trips() {
    let envelope = chunk_with_time_in_slot(7, JSON_SAFE_MAX);
    let bytes = encoded(&envelope);
    assert_ne!(bytes.len(), 0, "nothing encoded");
    assert_eq!(decode_message(&bytes), Ok(envelope));
}

#[test]
fn an_event_time_over_the_json_limit_is_rejected_in_any_slot() {
    for slot in 0..EVENTS_PER_CHUNK {
        let envelope = chunk_with_time_in_slot(slot, JSON_SAFE_MAX + 1);
        let mut buf = [0u8; MAX_PAYLOAD];
        assert_eq!(
            encode_message(&envelope, &mut buf),
            Err(MessageError::OutOfRange),
            "encode, slot {slot}"
        );
        let raw = postcard::to_slice(&envelope, &mut buf).unwrap();
        assert_eq!(
            decode_message(raw),
            Err(MessageError::OutOfRange),
            "decode, slot {slot}"
        );
    }
}

#[test]
fn inconsistent_page_and_chunk_headers_fail_in_both_directions() {
    let mut page = two_alarm_page();
    if let Message::AlarmPage(ref mut p) = page.message {
        p.total = 0;
    }
    let mut chunk = two_event_chunk();
    if let Message::EventLogChunk(ref mut c) = chunk.message {
        c.first_index += 1;
    }
    for bad in [page, chunk] {
        let mut buffer = [0; MAX_PAYLOAD];
        assert!(encode_message(&bad, &mut buffer).is_err());
        let bytes = postcard::to_slice(&bad, &mut buffer).unwrap();
        assert!(decode_message(bytes).is_err());
    }
}
