use elevator_wire::protocol::{
    CallFloor, CarMode, CarSnapshot, CommandResult, CommandStatus, Direction, DoorState, Envelope,
    Hello, JSON_SAFE_MAX, Message, MessageError, Rejection, ResetReason, decode_message,
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
