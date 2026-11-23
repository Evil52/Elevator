use elevator_gateway::{Bridge, CommandError, Stats, to_json};
use elevator_wire::protocol::{
    AckAlarm, CallFloor, CarMode, CarSnapshot, Direction, DoorState, Envelope, Hello, InjectFault,
    Message, MessageError, Ping, ReadLog, RecoverCar, ResetReason, SimAction, SimControl,
    decode_message, encode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, decode_frame, encode_frame};

fn hello(boot: u32, run: u32, seq: u32) -> Envelope {
    Envelope::new(
        boot,
        run,
        seq,
        Message::Hello(Hello {
            device_id: 7,
            reset_reason: ResetReason::PowerOn,
        }),
    )
}

fn snapshot(seq: u32) -> Envelope {
    Envelope::new(
        1,
        1,
        seq,
        Message::CarSnapshot(CarSnapshot {
            car: 0,
            mode: CarMode::Normal,
            doors: DoorState::Closed,
            floor: Some(1),
            position_mm: 0,
            speed_mm_per_sec: 0,
            eta_ms: None,
            up_calls: 0,
            down_calls: 0,
            model_time_ms: 5,
        }),
    )
}

fn frame_of(frame_seq: u8, envelope: &Envelope) -> Vec<u8> {
    let mut payload = [0_u8; MAX_PAYLOAD];
    let length = encode_message(envelope, &mut payload).expect("message");
    let mut out = [0_u8; MAX_FRAME];
    let written = encode_frame(frame_seq, &payload[..length], &mut out).expect("frame");
    out[..written].to_vec()
}

#[test]
fn a_frame_from_the_board_becomes_an_envelope() {
    let mut bridge = Bridge::new();
    let sent = hello(10, 2, 0);
    let got = bridge.feed(&frame_of(0, &sent));
    assert_eq!(got, vec![sent]);
    assert_eq!(bridge.stats().frames, 1);
    assert_eq!(bridge.identity(), Some((10, 2)));
}

#[test]
fn bytes_may_arrive_one_at_a_time() {
    let mut bridge = Bridge::new();
    let sent = snapshot(3);
    let mut got = Vec::new();
    for byte in frame_of(5, &sent) {
        got.extend(bridge.feed(&[byte]));
    }
    assert_eq!(got, vec![sent]);
}

#[test]
fn several_frames_in_one_read_come_out_in_order() {
    let mut bridge = Bridge::new();
    let mut blob = frame_of(0, &hello(1, 1, 0));
    blob.extend(frame_of(1, &snapshot(1)));
    blob.extend(frame_of(2, &snapshot(2)));
    let got = bridge.feed(&blob);
    assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(bridge.stats().frames, 3);
}

#[test]
fn a_corrupted_frame_is_counted_and_the_next_one_still_decodes() {
    let mut bridge = Bridge::new();
    let mut bad = frame_of(0, &snapshot(0));
    bad[3] ^= 0x55;
    let mut blob = bad;
    blob.extend(frame_of(1, &snapshot(1)));
    let got = bridge.feed(&blob);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].seq, 1);
    assert_eq!(bridge.stats().bad_frames, 1);
    assert_eq!(bridge.stats().frames, 1);
}

#[test]
fn stray_delimiters_are_ignored() {
    let mut bridge = Bridge::new();
    assert_eq!(bridge.feed(&[0, 0, 0]), Vec::<Envelope>::new());
    assert_eq!(bridge.stats(), Stats::default());
}

#[test]
fn skipped_frame_numbers_count_as_lost_frames_and_wrap_correctly() {
    let mut bridge = Bridge::new();
    bridge.feed(&frame_of(250, &snapshot(0)));
    bridge.feed(&frame_of(251, &snapshot(1)));
    assert_eq!(bridge.stats().lost_frames, 0);
    bridge.feed(&frame_of(254, &snapshot(2)));
    assert_eq!(bridge.stats().lost_frames, 2);
    bridge.feed(&frame_of(255, &snapshot(3)));
    bridge.feed(&frame_of(0, &snapshot(4)));
    assert_eq!(bridge.stats().lost_frames, 2);
    assert_eq!(bridge.stats().frames, 5);
}

#[test]
fn a_valid_frame_with_a_broken_message_is_a_bad_message() {
    let mut bridge = Bridge::new();
    let mut out = [0_u8; MAX_FRAME];
    let written = encode_frame(0, &[9, 9, 9, 9], &mut out).expect("frame");
    assert_eq!(bridge.feed(&out[..written]), Vec::<Envelope>::new());
    assert_eq!(bridge.stats().bad_messages, 1);
    assert_eq!(bridge.stats().bad_frames, 0);
}

#[test]
fn the_identity_follows_the_latest_envelope() {
    let mut bridge = Bridge::new();
    assert_eq!(bridge.identity(), None);
    bridge.feed(&frame_of(0, &hello(1, 1, 0)));
    bridge.feed(&frame_of(1, &hello(1, 2, 1)));
    assert_eq!(bridge.identity(), Some((1, 2)));
    bridge.feed(&frame_of(2, &hello(9, 1, 0)));
    assert_eq!(bridge.identity(), Some((9, 1)));
}

#[test]
fn json_follows_the_contract_naming() {
    let json = to_json(&hello(10, 2, 4)).expect("json");
    assert_eq!(
        json,
        r#"{"version":1,"bootId":10,"runId":2,"seq":4,"message":{"hello":{"deviceId":7,"resetReason":"powerOn"}}}"#
    );
    let back: Envelope = serde_json::from_str(&json).expect("round trip");
    assert_eq!(back, hello(10, 2, 4));
}

fn command_json(request_id: u32) -> String {
    to_json(&Envelope::new(
        10,
        2,
        request_id,
        Message::CallFloor(CallFloor {
            request_id,
            floor: 3,
            direction: Direction::Down,
        }),
    ))
    .expect("json")
}

fn unframe(bytes: &[u8]) -> (u8, Envelope) {
    let mut payload = [0_u8; MAX_PAYLOAD];
    assert_eq!(bytes.last(), Some(&0));
    let (seq, length) = decode_frame(&bytes[..bytes.len() - 1], &mut payload).expect("frame");
    (seq, decode_message(&payload[..length]).expect("message"))
}

#[test]
fn a_client_command_becomes_a_frame_the_board_can_read() {
    let mut bridge = Bridge::new();
    let bytes = bridge.command_frame(&command_json(41)).expect("frame");
    let (frame_seq, envelope) = unframe(&bytes);
    assert_eq!(frame_seq, 0);
    assert_eq!(
        envelope.message,
        Message::CallFloor(CallFloor {
            request_id: 41,
            floor: 3,
            direction: Direction::Down
        })
    );
    assert_eq!((envelope.boot_id, envelope.run_id), (10, 2));
}

#[test]
fn outgoing_frame_numbers_count_up_and_wrap() {
    let mut bridge = Bridge::new();
    let numbers: Vec<u8> = (0..258)
        .map(|_| unframe(&bridge.command_frame(&command_json(1)).expect("frame")).0)
        .collect();
    assert_eq!(&numbers[..3], &[0, 1, 2]);
    assert_eq!(numbers[255], 255);
    assert_eq!(&numbers[256..], &[0, 1]);
}

#[test]
fn every_command_kind_is_forwarded() {
    let mut bridge = Bridge::new();
    let messages = [
        Message::CallFloor(CallFloor {
            request_id: 1,
            floor: 2,
            direction: Direction::Up,
        }),
        Message::AckAlarm(AckAlarm {
            request_id: 2,
            alarm_id: 1,
        }),
        Message::InjectFault(InjectFault {
            request_id: 3,
            car: 0,
        }),
        Message::RecoverCar(RecoverCar {
            request_id: 4,
            car: 0,
        }),
        Message::SimControl(SimControl {
            request_id: 5,
            action: SimAction::Pause,
        }),
        Message::ReadLog(ReadLog {
            request_id: 6,
            from_index: 0,
        }),
    ];
    for message in messages {
        let json = to_json(&Envelope::new(1, 1, 0, message)).expect("json");
        let bytes = bridge.command_frame(&json).expect("frame");
        assert_eq!(unframe(&bytes).1.message, message);
    }
}

#[test]
fn a_gap_across_the_wrap_is_counted_by_distance() {
    let mut bridge = Bridge::new();
    bridge.feed(&frame_of(254, &snapshot(0)));
    bridge.feed(&frame_of(1, &snapshot(1)));
    assert_eq!(bridge.stats().lost_frames, 2);
}

#[test]
fn telemetry_cannot_be_sent_to_the_board_by_a_client() {
    let mut bridge = Bridge::new();
    let json = to_json(&snapshot(0)).expect("json");
    assert!(matches!(
        bridge.command_frame(&json),
        Err(CommandError::NotACommand)
    ));
    let json = to_json(&hello(1, 1, 0)).expect("json");
    assert!(matches!(
        bridge.command_frame(&json),
        Err(CommandError::NotACommand)
    ));
}

#[test]
fn bad_commands_are_refused_and_consume_no_frame_number() {
    let mut bridge = Bridge::new();
    assert!(matches!(
        bridge.command_frame("not json"),
        Err(CommandError::Json(_))
    ));
    let unknown = command_json(1).replace("\"floor\":3", "\"floor\":3,\"extra\":1");
    assert!(matches!(
        bridge.command_frame(&unknown),
        Err(CommandError::Json(_))
    ));
    let wrong_version = command_json(1).replace("\"version\":1", "\"version\":2");
    assert!(matches!(
        bridge.command_frame(&wrong_version),
        Err(CommandError::Message(MessageError::BadVersion(2)))
    ));
    let bytes = bridge.command_frame(&command_json(2)).expect("frame");
    assert_eq!(unframe(&bytes).0, 0);
}

#[test]
fn ping_frames_carry_a_counter_that_counts_up_and_the_frame_number_advances() {
    let mut bridge = Bridge::new();
    let first = unframe(&bridge.ping_frame().expect("frame"));
    let second = unframe(&bridge.ping_frame().expect("frame"));
    assert_eq!(first.1.message, Message::Ping(Ping { counter: 0 }));
    assert_eq!(second.1.message, Message::Ping(Ping { counter: 1 }));
    assert_eq!((first.0, second.0), (0, 1));
}

#[test]
fn a_ping_frame_shares_the_frame_numbers_with_commands() {
    let mut bridge = Bridge::new();
    bridge.command_frame(&command_json(1)).expect("frame");
    assert_eq!(unframe(&bridge.ping_frame().expect("frame")).0, 1);
    assert_eq!(
        unframe(&bridge.command_frame(&command_json(2)).expect("frame")).0,
        2
    );
}

#[test]
fn a_ping_frame_names_the_board_it_was_last_heard_from() {
    let mut bridge = Bridge::new();
    let unheard = unframe(&bridge.ping_frame().expect("frame")).1;
    assert_eq!((unheard.boot_id, unheard.run_id), (0, 0));
    bridge.feed(&frame_of(0, &hello(77, 3, 1)));
    let heard = unframe(&bridge.ping_frame().expect("frame")).1;
    assert_eq!((heard.boot_id, heard.run_id), (77, 3));
}

#[test]
fn the_ping_counter_keeps_counting() {
    let mut bridge = Bridge::new();
    for _ in 0..3 {
        bridge.ping_frame().expect("frame");
    }
    let envelope = unframe(&bridge.ping_frame().expect("frame")).1;
    assert_eq!(envelope.message, Message::Ping(Ping { counter: 3 }));
}

#[test]
fn a_browser_cannot_send_a_ping() {
    let mut bridge = Bridge::new();
    let json = to_json(&Envelope::new(1, 1, 0, Message::Ping(Ping { counter: 0 }))).expect("json");
    assert!(matches!(
        bridge.command_frame(&json),
        Err(CommandError::NotACommand)
    ));
}
