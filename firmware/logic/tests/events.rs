use elevator_fw_logic::events::EventLog;
use elevator_wire::protocol::{
    EVENTS_PER_CHUNK, Envelope, Event, EventKind, EventLogChunk, Message,
};

fn log_with<const N: usize>(count: u32) -> EventLog<N> {
    let mut log = EventLog::<N>::new();
    for i in 0..count {
        log.push(u64::from(i) * 10, EventKind::Assigned, Some(1), i + 100);
    }
    log
}

fn indexes(chunk: &EventLogChunk) -> Vec<u32> {
    chunk
        .events
        .iter()
        .flatten()
        .map(|event| event.index)
        .collect()
}

fn valid(chunk: EventLogChunk) -> bool {
    Envelope::new(1, 1, 0, Message::EventLogChunk(chunk))
        .validate()
        .is_ok()
}

#[test]
fn an_empty_log_gives_an_empty_valid_chunk() {
    let log = EventLog::<4>::new();
    let chunk = log.chunk(0);
    assert_eq!((chunk.first_index, chunk.dropped_before), (0, 0));
    assert!(chunk.events.iter().all(Option::is_none));
    assert!(valid(chunk));
}

#[test]
fn pushed_events_are_numbered_from_zero_and_keep_their_fields() {
    let mut log = EventLog::<4>::new();
    log.push(1_000, EventKind::Served, Some(2), 77);
    log.push(2_000, EventKind::AlarmRaised, None, 5);
    let chunk = log.chunk(0);
    assert_eq!(
        chunk.events[0],
        Some(Event {
            index: 0,
            at_ms: 1_000,
            kind: EventKind::Served,
            car: Some(2),
            id: 77
        })
    );
    assert_eq!(
        chunk.events[1],
        Some(Event {
            index: 1,
            at_ms: 2_000,
            kind: EventKind::AlarmRaised,
            car: None,
            id: 5
        })
    );
    assert_eq!(chunk.events[2], None);
    assert_eq!(log.next_index(), 2);
}

#[test]
fn reading_starts_at_the_requested_index() {
    let log = log_with::<8>(5);
    assert_eq!(indexes(&log.chunk(2)), vec![2, 3, 4]);
    assert_eq!(log.chunk(2).first_index, 2);
    let end = log.chunk(5);
    assert_eq!(end.first_index, 5);
    assert!(end.events.iter().all(Option::is_none));
    let beyond = log.chunk(99);
    assert_eq!(beyond.first_index, 5);
    assert!(beyond.events.iter().all(Option::is_none));
    assert!(valid(beyond));
}

#[test]
fn a_chunk_holds_at_most_eight_events_and_the_next_one_continues() {
    let log = log_with::<32>(12);
    let first = log.chunk(0);
    assert_eq!(indexes(&first), (0..8).collect::<Vec<_>>());
    assert_eq!(EVENTS_PER_CHUNK, 8);
    let second = log.chunk(8);
    assert_eq!(indexes(&second), vec![8, 9, 10, 11]);
    assert!(valid(first) && valid(second));
}

#[test]
fn overflow_drops_the_oldest_and_reports_how_many_the_reader_missed() {
    let log = log_with::<4>(6);
    assert_eq!(log.oldest_index(), 2);
    let chunk = log.chunk(0);
    assert_eq!((chunk.first_index, chunk.dropped_before), (2, 2));
    assert_eq!(indexes(&chunk), vec![2, 3, 4, 5]);
    let inside = log.chunk(3);
    assert_eq!((inside.first_index, inside.dropped_before), (3, 0));
    assert_eq!(indexes(&inside), vec![3, 4, 5]);
    assert!(valid(chunk) && valid(inside));
}

#[test]
fn push_reports_an_overflow_only_when_it_overwrites() {
    let mut log = EventLog::<3>::new();
    let flags: Vec<bool> = (0..5)
        .map(|i| log.push(i, EventKind::Assigned, None, 0))
        .collect();
    assert_eq!(flags, vec![false, false, false, true, true]);
}

#[test]
fn indexes_never_repeat_after_wrapping_around_the_storage() {
    let log = log_with::<3>(10);
    assert_eq!(indexes(&log.chunk(0)), vec![7, 8, 9]);
    assert_eq!(log.next_index(), 10);
}
