use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use elevator_gateway::link::supervise;
use elevator_gateway::{Gateway, OriginPolicy, to_json};
use elevator_wire::protocol::{
    CallFloor, CarMode, CarSnapshot, Direction, DoorState, Envelope, Message, encode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, encode_frame};
use serde_json::Value;

enum Step {
    Data(Vec<u8>),
    TimedOut,
    Fail,
    Eof,
}

struct ScriptedReader(VecDeque<Step>);

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.pop_front() {
            Some(Step::Data(bytes)) => {
                buf[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            Some(Step::TimedOut) => Err(io::ErrorKind::TimedOut.into()),
            Some(Step::Eof) => Ok(0),
            Some(Step::Fail) | None => Err(io::Error::other("unplugged")),
        }
    }
}

#[derive(Clone, Default)]
struct Wire(Arc<Mutex<Vec<u8>>>);

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("wire").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn snapshot(time: u64) -> Envelope {
    Envelope::new(
        4,
        1,
        1,
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
            model_time_ms: time,
        }),
    )
}

fn frame(seq: u8, envelope: &Envelope) -> Vec<u8> {
    let mut payload = [0_u8; MAX_PAYLOAD];
    let length = encode_message(envelope, &mut payload).expect("message");
    let mut out = [0_u8; MAX_FRAME];
    let written = encode_frame(seq, &payload[..length], &mut out).expect("frame");
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

fn gateway() -> Arc<Gateway> {
    Gateway::new(Box::new(io::sink()), OriginPolicy::new(Vec::new(), true))
}

fn journal_kinds(gateway: &Gateway) -> Vec<String> {
    gateway
        .export_journal()
        .lines()
        .skip(1)
        .map(|line| {
            let entry: Value = serde_json::from_str(line).expect("json");
            match entry["dir"].as_str().expect("dir") {
                "link" => format!("link:{}", entry["state"].as_str().expect("state")),
                other => other.to_owned(),
            }
        })
        .collect()
}

type Opened = io::Result<(ScriptedReader, Wire)>;

fn run(
    gateway: &Gateway,
    mut attempts: VecDeque<Opened>,
    stop_after_opens: usize,
) -> (Vec<Duration>, Vec<String>) {
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&opens);
    let mut sleeps = Vec::new();
    let mut lines = Vec::new();
    supervise(
        gateway,
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            attempts
                .pop_front()
                .unwrap_or_else(|| Err(io::Error::other("no more")))
        },
        |pause| sleeps.push(pause),
        || opens.load(Ordering::SeqCst) >= stop_after_opens,
        |batch| lines.extend(batch.iter().cloned()),
    );
    (sleeps, lines)
}

fn session(steps: Vec<Step>) -> (Opened, Wire) {
    let wire = Wire::default();
    (Ok((ScriptedReader(steps.into()), wire.clone())), wire)
}

#[test]
fn frames_from_the_first_connection_reach_the_gateway() {
    let gateway = gateway();
    let (first, _) = session(vec![Step::Data(frame(0, &snapshot(1))), Step::Fail]);
    let (_, lines) = run(&gateway, VecDeque::from([first]), 2);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"modelTimeMs\":1"));
}

#[test]
fn a_lost_port_stops_commands_and_a_new_one_gets_them() {
    let gateway = gateway();
    let (first, first_wire) = session(vec![Step::Fail]);
    let (second, second_wire) = session(vec![Step::TimedOut, Step::Fail]);
    let sent_while_down = Arc::new(Mutex::new(None));
    let probe = Arc::clone(&sent_while_down);
    let probe_gateway = Arc::clone(&gateway);
    let mut attempts = VecDeque::from([first, second]);
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&opens);
    supervise(
        &gateway,
        move || {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            if n == 1 {
                *probe.lock().expect("probe") = Some(probe_gateway.command(&call_json()));
            }
            attempts
                .pop_front()
                .unwrap_or_else(|| Err(io::Error::other("none")))
        },
        |_| {},
        || opens.load(Ordering::SeqCst) >= 3,
        |_| {},
    );
    assert_eq!(*sent_while_down.lock().expect("probe"), Some(false));
    assert!(first_wire.0.lock().expect("wire").is_empty());
    assert!(second_wire.0.lock().expect("wire").is_empty());
    assert!(!gateway.command(&call_json()));
}

#[test]
fn a_command_reaches_the_current_port_while_it_is_connected() {
    let gateway = gateway();
    let wire = Wire::default();
    let reader = ScriptedReader(VecDeque::from([Step::TimedOut]));
    let sent = Arc::new(Mutex::new(false));
    let flag = Arc::clone(&sent);
    let probe_gateway = Arc::clone(&gateway);
    let reads = Arc::new(AtomicUsize::new(0));
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&opens);
    let mut once = Some((reader, wire.clone()));
    let probe_reads = Arc::clone(&reads);
    supervise(
        &gateway,
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            once.take().ok_or_else(|| io::Error::other("done"))
        },
        |_| {},
        || {
            probe_reads.fetch_add(1, Ordering::SeqCst);
            if probe_reads.load(Ordering::SeqCst) == 2 {
                *flag.lock().expect("flag") = probe_gateway.command(&call_json());
            }
            opens.load(Ordering::SeqCst) >= 2
        },
        |_| {},
    );
    assert!(*sent.lock().expect("flag"));
    assert!(!wire.0.lock().expect("wire").is_empty());
}

#[test]
fn timeouts_are_quiet_and_do_not_count_as_a_loss() {
    let gateway = gateway();
    let (first, _) = session(vec![
        Step::TimedOut,
        Step::TimedOut,
        Step::Data(frame(0, &snapshot(1))),
        Step::TimedOut,
        Step::Fail,
    ]);
    let (_, lines) = run(&gateway, VecDeque::from([first]), 2);
    assert_eq!(lines.len(), 1);
    assert_eq!(journal_kinds(&gateway), vec!["link:up", "rx", "link:down"]);
}

#[test]
fn a_closed_stream_counts_as_a_loss() {
    let gateway = gateway();
    let (first, _) = session(vec![Step::Eof, Step::Data(frame(0, &snapshot(1)))]);
    let (_, lines) = run(&gateway, VecDeque::from([first]), 2);
    assert_eq!(journal_kinds(&gateway), vec!["link:up", "link:down"]);
    assert_eq!(lines, Vec::<String>::new());
}

#[test]
fn failed_opens_back_off_with_doubling_and_a_cap_and_success_resets() {
    let gateway = gateway();
    let fail = || Err(io::Error::other("no device"));
    let (good, _) = session(vec![Step::Fail]);
    let attempts = VecDeque::from([fail(), fail(), fail(), fail(), fail(), fail(), good, fail()]);
    let (sleeps, _) = run(&gateway, attempts, 9);
    let ms: Vec<u64> = sleeps
        .iter()
        .map(|d| u64::try_from(d.as_millis()).expect("ms"))
        .collect();
    assert_eq!(&ms[..6], &[500, 1000, 2000, 4000, 5000, 5000]);
    assert_eq!(ms[6], 500);
    assert_eq!(ms[7], 500);
}

#[test]
fn a_partial_frame_before_the_drop_does_not_corrupt_the_next_connection() {
    let gateway = gateway();
    let whole = frame(0, &snapshot(1));
    let half = whole[..whole.len() / 2].to_vec();
    let (first, _) = session(vec![Step::Data(half), Step::Fail]);
    let (second, _) = session(vec![Step::Data(frame(7, &snapshot(2))), Step::Fail]);
    let (_, lines) = run(&gateway, VecDeque::from([first, second]), 3);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"modelTimeMs\":2"));
    assert_eq!(gateway.stats().lost_frames, 0);
    assert_eq!(gateway.stats().bad_frames, 0);
}

#[test]
fn frame_numbers_may_restart_after_a_reconnect_without_counting_losses() {
    let gateway = gateway();
    let (first, _) = session(vec![Step::Data(frame(200, &snapshot(1))), Step::Fail]);
    let (second, _) = session(vec![Step::Data(frame(3, &snapshot(2))), Step::Fail]);
    run(&gateway, VecDeque::from([first, second]), 3);
    assert_eq!(gateway.stats().lost_frames, 0);
}

#[test]
fn the_loop_ends_when_asked_to_stop() {
    let gateway = gateway();
    let (sleeps, lines) = run(&gateway, VecDeque::new(), 0);
    assert_eq!(sleeps, Vec::<Duration>::new());
    assert_eq!(lines, Vec::<String>::new());
}

#[test]
fn the_journal_records_every_up_and_down_in_order() {
    let gateway = gateway();
    let (first, _) = session(vec![Step::Fail]);
    let (second, _) = session(vec![Step::Fail]);
    run(&gateway, VecDeque::from([first, second]), 3);
    assert_eq!(
        journal_kinds(&gateway),
        vec!["link:up", "link:down", "link:up", "link:down"]
    );
}

#[test]
fn the_line_callback_is_not_called_for_reads_that_produce_nothing() {
    let gateway = gateway();
    let whole = frame(0, &snapshot(1));
    let (first, _) = session(vec![
        Step::Data(whole[..3].to_vec()),
        Step::Data(whole[3..].to_vec()),
        Step::Fail,
    ]);
    let mut attempts = VecDeque::from([first]);
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&opens);
    let mut batches = Vec::new();
    supervise(
        &gateway,
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            attempts
                .pop_front()
                .unwrap_or_else(|| Err(io::Error::other("none")))
        },
        |_| {},
        || opens.load(Ordering::SeqCst) >= 2,
        |batch| batches.push(batch.len()),
    );
    assert_eq!(batches, vec![1]);
}
