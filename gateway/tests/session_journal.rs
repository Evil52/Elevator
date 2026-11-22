use std::io::Write;
use std::sync::{Arc, Mutex};

use elevator_gateway::{Gateway, Journal, OriginPolicy, to_json};
use elevator_wire::protocol::{
    CallFloor, CarMode, CarSnapshot, Direction, DoorState, Envelope, Message, encode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, encode_frame};
use serde_json::Value;

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Broken;

impl Write for Broken {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("unplugged"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn gateway(writer: Box<dyn Write + Send>) -> Arc<Gateway> {
    Gateway::new(writer, OriginPolicy::new(Vec::new(), true))
}

fn snapshot(time: u64, mode: CarMode) -> Envelope {
    Envelope::new(
        4,
        1,
        1,
        Message::CarSnapshot(CarSnapshot {
            car: 0,
            mode,
            doors: DoorState::Closed,
            floor: Some(1),
            position_mm: 0,
            speed_mm_per_sec: 0,
            eta_ms: None,
            up_calls: 0,
            down_calls: 0,
            model_time_ms: time,
        }),
    )
}

fn board_frame(envelope: &Envelope) -> Vec<u8> {
    let mut payload = [0_u8; MAX_PAYLOAD];
    let length = encode_message(envelope, &mut payload).expect("message");
    let mut out = [0_u8; MAX_FRAME];
    let written = encode_frame(0, &payload[..length], &mut out).expect("frame");
    out[..written].to_vec()
}

fn call_json() -> String {
    to_json(&Envelope::new(
        4,
        1,
        9,
        Message::CallFloor(CallFloor {
            request_id: 5,
            floor: 3,
            direction: Direction::Down,
        }),
    ))
    .expect("json")
}

fn export(gateway: &Gateway) -> Vec<Value> {
    gateway
        .export_journal()
        .lines()
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect()
}

#[test]
fn telemetry_from_the_board_is_journaled_as_rx_with_a_real_time() {
    let gateway = gateway(Box::new(Sink::default()));
    gateway.ingest(&board_frame(&snapshot(1, CarMode::Normal)));
    let all = export(&gateway);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1]["dir"], "rx");
    assert_eq!(all[1]["session"], Value::Null);
    assert!(all[1]["t"].as_u64().expect("time") > 1_600_000_000_000);
    assert_eq!(all[1]["envelope"]["message"]["carSnapshot"]["car"], 0);
}

#[test]
fn identical_snapshots_are_journaled_once() {
    let gateway = gateway(Box::new(Sink::default()));
    for time in 1..=5 {
        gateway.ingest(&board_frame(&snapshot(time, CarMode::Normal)));
    }
    gateway.ingest(&board_frame(&snapshot(6, CarMode::Unavailable)));
    assert_eq!(export(&gateway).len(), 3);
}

#[test]
fn a_sent_command_is_journaled_as_tx_with_its_session() {
    let sink = Sink::default();
    let gateway = gateway(Box::new(sink.clone()));
    assert!(gateway.command_from(5, &call_json()));
    assert!(!sink.0.lock().expect("sink").is_empty());
    let all = export(&gateway);
    assert_eq!(all[1]["dir"], "tx");
    assert_eq!(all[1]["session"], 5);
    assert_eq!(all[1]["envelope"]["message"]["callFloor"]["requestId"], 5);
}

#[test]
fn refused_commands_are_journaled_with_a_reason_and_without_the_payload() {
    let gateway = gateway(Box::new(Sink::default()));
    let spoof = to_json(&snapshot(1, CarMode::Normal)).expect("json");
    assert!(!gateway.command_from(6, &spoof));
    assert!(!gateway.command_from(6, "{broken"));
    let wrong = call_json().replace("\"version\":1", "\"version\":9");
    assert!(!gateway.command_from(7, &wrong));
    let all = export(&gateway);
    let reasons: Vec<(&str, u64)> = all[1..]
        .iter()
        .map(|e| {
            (
                e["reason"].as_str().expect("reason"),
                e["session"].as_u64().expect("session"),
            )
        })
        .collect();
    assert_eq!(
        reasons,
        vec![("notACommand", 6), ("json", 6), ("invalid", 7)]
    );
    assert!(
        all[1..]
            .iter()
            .all(|e| e["dir"] == "refused" && e.get("envelope").is_none())
    );
    assert_eq!(gateway.refused(), 3);
}

#[test]
fn a_dead_serial_line_is_a_refusal_with_its_own_reason() {
    let gateway = gateway(Box::new(Broken));
    assert!(!gateway.command_from(1, &call_json()));
    let all = export(&gateway);
    assert_eq!(all[1]["reason"], "serial");
    assert_eq!(all.len(), 2);
}

#[test]
fn the_cli_path_uses_session_zero() {
    let gateway = gateway(Box::new(Sink::default()));
    assert!(gateway.command(&call_json()));
    assert_eq!(export(&gateway)[1]["session"], 0);
}

#[test]
fn a_small_journal_reports_what_it_dropped() {
    let gateway = Gateway::with_journal(
        Box::new(Sink::default()),
        OriginPolicy::new(Vec::new(), true),
        Journal::new(2),
    );
    for _ in 0..5 {
        gateway.command_from(1, &call_json());
    }
    let all = export(&gateway);
    assert_eq!(all[0]["journal"]["dropped"], 3);
    assert_eq!(all.len(), 3);
}
