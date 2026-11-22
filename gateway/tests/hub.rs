use elevator_gateway::Hub;
use elevator_wire::protocol::{
    Alarm, AlarmKind, AlarmPage, AlarmState, CarMode, CarSnapshot, CommandResult, CommandStatus,
    DoorState, Envelope, Hello, Message, ResetReason,
};

fn env(message: Message) -> Envelope {
    Envelope::new(1, 1, 0, message)
}

fn snapshot(car: u8, time: u64) -> Envelope {
    env(Message::CarSnapshot(CarSnapshot {
        car,
        mode: CarMode::Normal,
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

fn page(offset: u16, id: u32) -> Envelope {
    let mut items = [None; 8];
    items[0] = Some(Alarm {
        id,
        kind: AlarmKind::CarFault,
        car: Some(0),
        state: AlarmState::Active,
        raised_at_ms: 1,
    });
    env(Message::AlarmPage(AlarmPage {
        total: offset + 1,
        offset,
        items,
    }))
}

fn publish(hub: &Hub, envelope: &Envelope, tag: &str) {
    hub.publish(envelope, tag);
}

#[test]
fn published_lines_reach_every_subscriber_in_order() {
    let hub = Hub::new();
    let (_, first) = hub.subscribe();
    let (_, second) = hub.subscribe();
    publish(&hub, &snapshot(0, 1), "a");
    publish(&hub, &snapshot(0, 2), "b");
    for receiver in [&first, &second] {
        assert_eq!(receiver.try_recv().ok(), Some("a".to_owned()));
        assert_eq!(receiver.try_recv().ok(), Some("b".to_owned()));
    }
    assert_eq!(hub.clients(), 2);
}

#[test]
fn a_departed_subscriber_is_dropped_without_hurting_the_others() {
    let hub = Hub::new();
    let (_, gone) = hub.subscribe();
    let (_, stays) = hub.subscribe();
    drop(gone);
    publish(&hub, &snapshot(0, 1), "a");
    assert_eq!(stays.try_recv().ok(), Some("a".to_owned()));
    assert_eq!(hub.clients(), 1);
}

#[test]
fn a_new_subscriber_gets_the_latest_state_first() {
    let hub = Hub::new();
    publish(&hub, &snapshot(0, 1), "car0-old");
    publish(&hub, &snapshot(0, 2), "car0-new");
    publish(&hub, &snapshot(1, 2), "car1");
    publish(&hub, &page(0, 1), "page0-old");
    publish(&hub, &page(0, 2), "page0-new");
    publish(&hub, &page(8, 3), "page8");
    publish(
        &hub,
        &env(Message::Hello(Hello {
            device_id: 1,
            reset_reason: ResetReason::PowerOn,
        })),
        "hello",
    );
    let (replay, _) = hub.subscribe();
    let mut sorted = replay.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["car0-new", "car1", "hello", "page0-new", "page8"]
    );
}

#[test]
fn results_are_not_replayed_to_late_subscribers() {
    let hub = Hub::new();
    publish(
        &hub,
        &env(Message::CommandResult(CommandResult {
            request_id: 1,
            status: CommandStatus::Applied,
        })),
        "result",
    );
    let (replay, _) = hub.subscribe();
    assert_eq!(replay, Vec::<String>::new());
}

#[test]
fn replay_and_live_lines_neither_repeat_nor_miss() {
    let hub = Hub::new();
    publish(&hub, &snapshot(0, 1), "before");
    let (replay, live) = hub.subscribe();
    publish(&hub, &snapshot(0, 2), "after");
    assert_eq!(replay, vec!["before"]);
    assert_eq!(live.try_recv().ok(), Some("after".to_owned()));
    assert_eq!(live.try_recv().ok(), None);
}

#[test]
fn a_new_reset_forgets_nothing_but_replaces_by_key() {
    let hub = Hub::new();
    publish(&hub, &snapshot(0, 1), "x");
    publish(&hub, &snapshot(0, 2), "y");
    let (replay, _) = hub.subscribe();
    assert_eq!(replay, vec!["y"]);
}
