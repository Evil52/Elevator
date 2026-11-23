use elevator_fw_logic::link_watch::{LINK_TIMEOUT_MS, LinkMonitor};

#[test]
fn a_new_monitor_reports_no_link() {
    let mut monitor = LinkMonitor::new();
    assert!(!monitor.update(0, 0));
    assert!(!monitor.update(10_000, 0));
}

#[test]
fn the_first_frame_brings_the_link_up() {
    let mut monitor = LinkMonitor::new();
    assert!(!monitor.update(100, 0));
    assert!(monitor.update(110, 1));
}

#[test]
fn the_link_stays_up_until_the_timeout_counted_from_the_last_frame() {
    let mut monitor = LinkMonitor::new();
    monitor.update(1000, 1);
    assert!(monitor.update(1000 + LINK_TIMEOUT_MS - 1, 1));
    assert!(!monitor.update(1000 + LINK_TIMEOUT_MS, 1));
}

#[test]
fn every_new_frame_restarts_the_timeout() {
    let mut monitor = LinkMonitor::new();
    monitor.update(0, 1);
    monitor.update(2500, 2);
    assert!(monitor.update(5000, 2));
    assert!(!monitor.update(5500, 2));
}

#[test]
fn the_link_comes_back_after_a_silence() {
    let mut monitor = LinkMonitor::new();
    monitor.update(0, 1);
    assert!(!monitor.update(10_000, 1));
    assert!(monitor.update(10_010, 2));
    assert!(monitor.update(10_020, 2));
}

#[test]
fn the_frame_counter_wrapping_around_still_counts_as_a_frame() {
    let mut monitor = LinkMonitor::new();
    monitor.update(0, u32::MAX);
    assert!(!monitor.update(5000, u32::MAX));
    assert!(monitor.update(5010, 0));
}

#[test]
fn a_time_that_goes_backwards_does_not_panic_or_expire_the_link() {
    let mut monitor = LinkMonitor::new();
    monitor.update(10_000, 1);
    assert!(monitor.update(5_000, 1));
}

#[test]
fn the_timeout_is_three_seconds_three_missed_pings() {
    assert_eq!(LINK_TIMEOUT_MS, 3000);
}

#[test]
fn only_a_ping_is_kept_away_from_the_control_task() {
    use elevator_fw_logic::link_watch::needs_control;
    use elevator_wire::protocol::{CallFloor, Direction, Envelope, Message, Ping, ReadLog};
    let ping = Envelope::new(1, 1, 1, Message::Ping(Ping { counter: 3 }));
    let call = Envelope::new(
        1,
        1,
        2,
        Message::CallFloor(CallFloor {
            request_id: 1,
            floor: 2,
            direction: Direction::Up,
        }),
    );
    let read = Envelope::new(
        1,
        1,
        3,
        Message::ReadLog(ReadLog {
            request_id: 1,
            from_index: 0,
        }),
    );
    assert!(!needs_control(&ping));
    assert!(needs_control(&call));
    assert!(needs_control(&read));
}
