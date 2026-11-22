use std::io::Write;
use std::sync::{Arc, Mutex};

use elevator_gateway::{Direction, Journal};
use elevator_wire::protocol::{
    Alarm, AlarmKind, AlarmPage, AlarmState, CarMode, CarSnapshot, CommandResult, CommandStatus,
    DoorState, Envelope, Hello, Message, ResetReason,
};
use serde_json::Value;

fn env(message: Message) -> Envelope {
    Envelope::new(1, 1, 0, message)
}

fn snapshot(car: u8, mode: CarMode, time: u64) -> Envelope {
    env(Message::CarSnapshot(CarSnapshot {
        car,
        mode,
        doors: DoorState::Closed,
        floor: Some(1),
        position_mm: 0,
        speed_mm_per_sec: 0,
        eta_ms: None,
        up_calls: 0,
        down_calls: 0,
        model_time_ms: time,
    }))
}

fn hello() -> Envelope {
    env(Message::Hello(Hello {
        device_id: 7,
        reset_reason: ResetReason::Watchdog,
    }))
}

fn result(id: u32) -> Envelope {
    env(Message::CommandResult(CommandResult {
        request_id: id,
        status: CommandStatus::Applied,
    }))
}

fn page() -> Envelope {
    let mut items = [None; 8];
    items[0] = Some(Alarm {
        id: 1,
        kind: AlarmKind::CarFault,
        car: Some(0),
        state: AlarmState::Active,
        raised_at_ms: 5,
    });
    env(Message::AlarmPage(AlarmPage {
        total: 1,
        offset: 0,
        items,
    }))
}

fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect()
}

#[test]
fn an_entry_carries_time_direction_session_and_the_envelope() {
    let mut journal = Journal::new(10);
    assert!(journal.record(1_000, Direction::Rx, None, &hello()));
    assert!(journal.record(2_000, Direction::Tx, Some(3), &result(9)));
    let all = lines(&journal.export(5_000));
    let first = &all[1];
    assert_eq!(first["t"], 1_000);
    assert_eq!(first["dir"], "rx");
    assert_eq!(first["session"], Value::Null);
    assert_eq!(first["envelope"]["message"]["hello"]["deviceId"], 7);
    assert_eq!(
        first["envelope"]["message"]["hello"]["resetReason"],
        "watchdog"
    );
    let second = &all[2];
    assert_eq!(
        (second["t"].as_u64(), second["dir"].as_str()),
        (Some(2_000), Some("tx"))
    );
    assert_eq!(second["session"], 3);
}

#[test]
fn the_export_starts_with_a_header_and_ends_with_a_newline() {
    let mut journal = Journal::new(10);
    journal.record(1, Direction::Rx, None, &hello());
    let text = journal.export(77);
    assert!(text.ends_with('\n'));
    let all = lines(&text);
    assert_eq!(all[0]["t"], 77);
    assert_eq!(all[0]["journal"]["dropped"], 0);
    assert_eq!(all[0]["journal"]["entries"], 1);
    assert_eq!(all.len(), 2);
}

#[test]
fn an_empty_journal_still_exports_its_header() {
    let journal = Journal::new(4);
    let all = lines(&journal.export(1));
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["journal"]["entries"], 0);
}

#[test]
fn an_unchanged_snapshot_is_recorded_once_even_as_model_time_moves() {
    let mut journal = Journal::new(10);
    assert!(journal.record(1, Direction::Rx, None, &snapshot(0, CarMode::Normal, 100)));
    assert!(!journal.record(2, Direction::Rx, None, &snapshot(0, CarMode::Normal, 200)));
    assert!(!journal.record(3, Direction::Rx, None, &snapshot(0, CarMode::Normal, 300)));
    assert_eq!(journal.len(), 1);
}

#[test]
fn a_changed_snapshot_is_recorded_and_each_car_is_tracked_separately() {
    let mut journal = Journal::new(10);
    assert!(journal.record(1, Direction::Rx, None, &snapshot(0, CarMode::Normal, 1)));
    assert!(journal.record(2, Direction::Rx, None, &snapshot(1, CarMode::Normal, 1)));
    assert!(journal.record(
        3,
        Direction::Rx,
        None,
        &snapshot(0, CarMode::Unavailable, 2)
    ));
    assert!(journal.record(4, Direction::Rx, None, &snapshot(0, CarMode::Normal, 3)));
    assert_eq!(journal.len(), 4);
}

#[test]
fn everything_that_is_not_a_snapshot_is_always_recorded() {
    let mut journal = Journal::new(10);
    for _ in 0..3 {
        assert!(journal.record(1, Direction::Rx, None, &hello()));
        assert!(journal.record(1, Direction::Rx, None, &page()));
        assert!(journal.record(1, Direction::Rx, None, &result(1)));
    }
    assert_eq!(journal.len(), 9);
}

#[test]
fn the_oldest_entries_are_dropped_and_counted_when_full() {
    let mut journal = Journal::new(3);
    for id in 0..5 {
        journal.record(u64::from(id), Direction::Rx, None, &result(id));
    }
    assert_eq!((journal.len(), journal.dropped()), (3, 2));
    let all = lines(&journal.export(9));
    assert_eq!(all[0]["journal"]["dropped"], 2);
    let ids: Vec<u64> = all[1..]
        .iter()
        .map(|e| {
            e["envelope"]["message"]["commandResult"]["requestId"]
                .as_u64()
                .expect("id")
        })
        .collect();
    assert_eq!(ids, vec![2, 3, 4]);
}

#[test]
fn a_refusal_is_recorded_with_its_reason_and_no_client_payload() {
    let mut journal = Journal::new(5);
    journal.record_refusal(10, 4, "notACommand");
    let all = lines(&journal.export(11));
    assert_eq!(all[1]["dir"], "refused");
    assert_eq!(all[1]["session"], 4);
    assert_eq!(all[1]["reason"], "notACommand");
    assert_eq!(all[1].get("envelope"), None);
}

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
        Err(std::io::Error::other("disk full"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_file_sink_gets_every_recorded_line_as_it_happens() {
    let sink = Sink::default();
    let mut journal = Journal::new(2).with_sink(Box::new(sink.clone()));
    for id in 0..4 {
        journal.record(u64::from(id), Direction::Rx, None, &result(id));
    }
    let written = String::from_utf8(sink.0.lock().expect("sink").clone()).expect("utf8");
    assert_eq!(lines(&written).len(), 4);
    assert!(written.ends_with('\n'));
    assert_eq!(journal.len(), 2);
}

#[test]
fn a_failing_sink_is_counted_and_never_stops_recording() {
    let mut journal = Journal::new(5).with_sink(Box::new(Broken));
    assert!(journal.record(1, Direction::Rx, None, &hello()));
    assert!(journal.record(2, Direction::Rx, None, &hello()));
    assert_eq!(journal.len(), 2);
    assert!(journal.sink_errors() >= 1);
}

#[test]
fn a_zero_capacity_journal_records_nothing_and_counts_the_loss() {
    let mut journal = Journal::new(0);
    assert!(!journal.record(1, Direction::Rx, None, &hello()));
    assert_eq!((journal.len(), journal.dropped()), (0, 1));
    assert!(journal.is_empty());
}
