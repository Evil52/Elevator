use elevator_fw_logic::reset::ResetCause;
use elevator_fw_logic::telemetry::{Telemetry, reset_reason};
use elevator_wire::protocol::{
    Alarm, AlarmKind, AlarmPage, AlarmState, CarMode, CarSnapshot, CommandResult, CommandStatus,
    DoorState, Envelope, Event, EventKind, EventLogChunk, Message, PROTOCOL_VERSION, ResetReason,
    decode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, decode_frame};

const DEVICE: u32 = 0xC0FF_EE01;
const BOOT: u32 = 0xB007_1D01;

fn open(frame: &[u8]) -> Result<(u8, Envelope), String> {
    assert_eq!(
        frame.last(),
        Some(&0),
        "frame must end with a zero delimiter"
    );
    assert!(
        !frame[..frame.len() - 1].contains(&0),
        "no zero byte inside a frame"
    );
    let mut payload = [0_u8; MAX_PAYLOAD];
    let (seq, len) = decode_frame(&frame[..frame.len() - 1], &mut payload)
        .map_err(|error| format!("frame: {error:?}"))?;
    let envelope =
        decode_message(&payload[..len]).map_err(|error| format!("message: {error:?}"))?;
    Ok((seq, envelope))
}

fn snapshot(car: u8) -> CarSnapshot {
    CarSnapshot {
        car,
        mode: CarMode::Normal,
        doors: DoorState::Closed,
        floor: Some(5),
        position_mm: 12_000,
        speed_mm_per_sec: 0,
        eta_ms: None,
        up_calls: 0,
        down_calls: 0,
        model_time_ms: 1_234,
    }
}

#[test]
fn hello_round_trips_through_the_frame_layer() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let len = telemetry
        .hello(ResetCause::PowerOn, &mut out)
        .expect("hello");
    let (frame_seq, envelope) = open(&out[..len]).expect("open");
    assert_eq!(frame_seq, 0);
    assert_eq!(envelope.version, PROTOCOL_VERSION);
    assert_eq!(envelope.boot_id, BOOT);
    assert_eq!(envelope.run_id, 1);
    assert_eq!(envelope.seq, 0);
    let Message::Hello(hello) = envelope.message else {
        panic!("expected hello");
    };
    assert_eq!(hello.device_id, DEVICE);
    assert_eq!(hello.reset_reason, ResetReason::PowerOn);
}

#[test]
fn a_snapshot_round_trips_and_continues_the_sequence() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    telemetry.hello(ResetCause::Pin, &mut out).expect("hello");
    let len = telemetry
        .car_snapshot(snapshot(2), &mut out)
        .expect("snapshot");
    let (frame_seq, envelope) = open(&out[..len]).expect("open");
    assert_eq!((frame_seq, envelope.seq), (1, 1));
    assert_eq!(envelope.message, Message::CarSnapshot(snapshot(2)));
}

#[test]
fn the_frame_counter_wraps_at_256_and_the_envelope_counter_does_not() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let mut last = (0, 0);
    for _ in 0..300 {
        let len = telemetry
            .car_snapshot(snapshot(0), &mut out)
            .expect("snapshot");
        let (frame_seq, envelope) = open(&out[..len]).expect("open");
        last = (frame_seq, envelope.seq);
    }
    assert_eq!(last, (43, 299));
}

#[test]
fn a_new_run_changes_only_the_run_id() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    telemetry
        .hello(ResetCause::PowerOn, &mut out)
        .expect("hello");
    telemetry.next_run();
    let len = telemetry
        .car_snapshot(snapshot(0), &mut out)
        .expect("snapshot");
    let (_, envelope) = open(&out[..len]).expect("open");
    assert_eq!(
        (envelope.boot_id, envelope.run_id, envelope.seq),
        (BOOT, 2, 1)
    );
}

#[test]
fn reset_causes_map_to_protocol_reasons() {
    let cases = [
        (ResetCause::IndependentWatchdog, ResetReason::Watchdog),
        (ResetCause::WindowWatchdog, ResetReason::Watchdog),
        (ResetCause::Software, ResetReason::Software),
        (ResetCause::PowerOn, ResetReason::PowerOn),
        (ResetCause::BrownOut, ResetReason::Brownout),
        (ResetCause::Pin, ResetReason::Button),
        (ResetCause::LowPower, ResetReason::Software),
        (ResetCause::Unknown, ResetReason::Software),
    ];
    for (cause, reason) in cases {
        assert_eq!(reset_reason(cause), reason, "{cause:?}");
    }
}

#[test]
fn the_reported_reason_is_the_mapped_cause() {
    let mut out = [0_u8; MAX_FRAME];
    for (cause, reason) in [
        (ResetCause::IndependentWatchdog, ResetReason::Watchdog),
        (ResetCause::Pin, ResetReason::Button),
    ] {
        let mut telemetry = Telemetry::new(DEVICE, BOOT);
        let len = telemetry.hello(cause, &mut out).expect("hello");
        let Message::Hello(hello) = open(&out[..len]).expect("open").1.message else {
            panic!("expected hello");
        };
        assert_eq!(hello.reset_reason, reason);
    }
}

#[test]
fn a_snapshot_the_protocol_rejects_is_an_error_and_consumes_no_number() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let bad = CarSnapshot {
        model_time_ms: u64::MAX,
        ..snapshot(0)
    };
    assert!(telemetry.car_snapshot(bad, &mut out).is_err());
    let len = telemetry
        .car_snapshot(snapshot(0), &mut out)
        .expect("snapshot");
    let (frame_seq, envelope) = open(&out[..len]).expect("open");
    assert_eq!((frame_seq, envelope.seq), (0, 0));
}

#[test]
fn a_command_result_round_trips_and_carries_the_current_run() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    telemetry.next_run();
    let mut out = [0_u8; MAX_FRAME];
    let result = CommandResult {
        request_id: 41,
        status: CommandStatus::Accepted,
    };
    let len = telemetry.command_result(result, &mut out).expect("result");
    let (_, envelope) = open(&out[..len]).expect("open");
    assert_eq!((envelope.boot_id, envelope.run_id), (BOOT, 2));
    assert_eq!(envelope.message, Message::CommandResult(result));
    assert_eq!((telemetry.boot_id(), telemetry.run_id()), (BOOT, 2));
}

#[test]
fn set_run_replaces_the_run_id_without_touching_the_counters() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    telemetry
        .hello(ResetCause::PowerOn, &mut out)
        .expect("hello");
    telemetry.set_run(9);
    let len = telemetry
        .car_snapshot(snapshot(0), &mut out)
        .expect("snapshot");
    let (frame_seq, envelope) = open(&out[..len]).expect("open");
    assert_eq!((envelope.run_id, envelope.seq, frame_seq), (9, 1, 1));
}

#[test]
fn an_alarm_page_round_trips_even_when_full_of_large_values() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let alarm = |id: u32| Alarm {
        id: u32::MAX - id,
        kind: AlarmKind::CarFault,
        car: Some(255),
        state: AlarmState::Acknowledged,
        raised_at_ms: (1 << 53) - 1,
    };
    let page = AlarmPage {
        total: 8,
        offset: 0,
        items: core::array::from_fn(|i| Some(alarm(u32::try_from(i).expect("small")))),
    };
    let len = telemetry.alarm_page(page, &mut out).expect("page");
    let (_, envelope) = open(&out[..len]).expect("open");
    assert_eq!(envelope.message, Message::AlarmPage(page));
}

#[test]
fn a_full_log_chunk_round_trips() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let event = |i: u32| Event {
        index: 4_000_000_000 + i,
        at_ms: (1 << 53) - 1,
        kind: EventKind::AlarmCleared,
        car: Some(255),
        id: u32::MAX,
    };
    let chunk = EventLogChunk {
        first_index: 4_000_000_000,
        dropped_before: u32::MAX,
        events: core::array::from_fn(|i| Some(event(u32::try_from(i).expect("small")))),
    };
    let len = telemetry.log_chunk(chunk, &mut out).expect("chunk");
    let (_, envelope) = open(&out[..len]).expect("open");
    assert_eq!(envelope.message, Message::EventLogChunk(chunk));
}

#[test]
fn an_inconsistent_page_is_refused_and_consumes_no_number() {
    let mut telemetry = Telemetry::new(DEVICE, BOOT);
    let mut out = [0_u8; MAX_FRAME];
    let bad = AlarmPage {
        total: 0,
        offset: 5,
        items: [None; 8],
    };
    assert!(telemetry.alarm_page(bad, &mut out).is_err());
    let len = telemetry
        .car_snapshot(snapshot(0), &mut out)
        .expect("snapshot");
    assert_eq!(open(&out[..len]).expect("open").1.seq, 0);
}
