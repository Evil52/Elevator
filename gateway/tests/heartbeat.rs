use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use elevator_gateway::link::{PING_EVERY, heartbeat};
use elevator_gateway::{Gateway, OriginPolicy};
use elevator_wire::protocol::{Message, Ping, decode_message};
use elevator_wire::{MAX_PAYLOAD, decode_frame};

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

struct BrokenWire;

impl Write for BrokenWire {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("unplugged"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn pings_on(wire: &Wire) -> Vec<u32> {
    let bytes = wire.0.lock().expect("wire").clone();
    bytes
        .split(|byte| *byte == 0)
        .filter(|frame| !frame.is_empty())
        .map(|frame| {
            let mut payload = [0_u8; MAX_PAYLOAD];
            let (_, length) = decode_frame(frame, &mut payload).expect("frame");
            match decode_message(&payload[..length]).expect("message").message {
                Message::Ping(Ping { counter }) => counter,
                other => panic!("unexpected {other:?}"),
            }
        })
        .collect()
}

fn gateway_with(wire: &Wire) -> Arc<Gateway> {
    Gateway::new(Box::new(wire.clone()), OriginPolicy::new(vec![], true))
}

#[test]
fn a_ping_reaches_the_board_as_a_readable_frame() {
    let wire = Wire::default();
    let gateway = gateway_with(&wire);
    assert!(gateway.ping_board());
    assert!(gateway.ping_board());
    assert_eq!(pings_on(&wire), [0, 1]);
}

#[test]
fn without_a_serial_link_nothing_is_sent_and_the_caller_is_told() {
    let wire = Wire::default();
    let gateway = gateway_with(&wire);
    gateway.set_writer(None);
    assert!(!gateway.ping_board());
    assert_eq!(pings_on(&wire).len(), 0);
}

#[test]
fn a_broken_serial_port_reports_failure() {
    let gateway = Gateway::new(Box::new(BrokenWire), OriginPolicy::new(vec![], true));
    assert!(!gateway.ping_board());
}

#[test]
fn pings_do_not_flood_the_journal_or_count_as_refused_commands() {
    let wire = Wire::default();
    let gateway = gateway_with(&wire);
    let before = gateway.export_journal().lines().count();
    for _ in 0..5 {
        gateway.ping_board();
    }
    assert_eq!(gateway.export_journal().lines().count(), before);
    assert_eq!(gateway.refused(), 0);
}

#[test]
fn the_heartbeat_pings_once_per_period_until_told_to_stop() {
    let wire = Wire::default();
    let gateway = gateway_with(&wire);
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let rounds = AtomicUsize::new(0);
    let log = Arc::clone(&sleeps);
    heartbeat(
        &gateway,
        PING_EVERY,
        move |period| log.lock().expect("sleeps").push(period),
        || rounds.fetch_add(1, Ordering::Relaxed) >= 3,
    );
    assert_eq!(pings_on(&wire), [0, 1, 2]);
    assert_eq!(*sleeps.lock().expect("sleeps"), vec![PING_EVERY; 3]);
}

#[test]
fn the_heartbeat_keeps_going_while_the_link_is_down() {
    let wire = Wire::default();
    let gateway = gateway_with(&wire);
    gateway.set_writer(None);
    let rounds = AtomicUsize::new(0);
    heartbeat(
        &gateway,
        Duration::from_millis(1),
        |_| {},
        || rounds.fetch_add(1, Ordering::Relaxed) >= 2,
    );
    gateway.set_writer(Some(Box::new(wire.clone())));
    assert!(gateway.ping_board());
    assert_eq!(pings_on(&wire), [2]);
}

#[test]
fn the_ping_period_is_one_second() {
    assert_eq!(PING_EVERY, Duration::from_secs(1));
}
