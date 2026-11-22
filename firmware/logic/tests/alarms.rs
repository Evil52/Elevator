use elevator_fw_logic::alarms::{AlarmError, AlarmTable, Raised};
use elevator_wire::protocol::{Alarm, AlarmKind, AlarmPage, AlarmState, Envelope, Message};

fn valid(page: AlarmPage) -> bool {
    Envelope::new(1, 1, 0, Message::AlarmPage(page))
        .validate()
        .is_ok()
}

fn ids(page: &AlarmPage) -> Vec<u32> {
    page.items.iter().flatten().map(|alarm| alarm.id).collect()
}

#[test]
fn a_new_alarm_is_active_numbered_from_one_and_remembers_its_time() {
    let mut table = AlarmTable::<4>::new();
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(2), 5_000),
        Ok(Raised::New(1))
    );
    assert_eq!(
        table.get(1),
        Some(Alarm {
            id: 1,
            kind: AlarmKind::CarFault,
            car: Some(2),
            state: AlarmState::Active,
            raised_at_ms: 5_000
        })
    );
    assert_eq!(table.total(), 1);
}

#[test]
fn raising_the_same_alarm_again_returns_the_existing_one() {
    let mut table = AlarmTable::<4>::new();
    table.raise(AlarmKind::CarFault, Some(2), 1).ok();
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(2), 9),
        Ok(Raised::Existing(1))
    );
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(1), 9),
        Ok(Raised::New(2))
    );
    assert_eq!(
        table.raise(AlarmKind::LinkLost, None, 9),
        Ok(Raised::New(3))
    );
    assert_eq!(table.total(), 3);
    assert_eq!(table.get(1).map(|a| a.raised_at_ms), Some(1));
}

#[test]
fn an_acknowledged_alarm_still_counts_as_raised() {
    let mut table = AlarmTable::<4>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    assert_eq!(table.acknowledge(1), Ok(()));
    assert_eq!(
        table.get(1).map(|a| a.state),
        Some(AlarmState::Acknowledged)
    );
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(0), 2),
        Ok(Raised::Existing(1))
    );
}

#[test]
fn acknowledging_names_its_failures() {
    let mut table = AlarmTable::<4>::new();
    assert_eq!(table.acknowledge(1), Err(AlarmError::NoSuchAlarm));
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    assert_eq!(table.acknowledge(1), Ok(()));
    assert_eq!(table.acknowledge(1), Err(AlarmError::WrongState));
    table.clear(AlarmKind::CarFault, Some(0));
    assert_eq!(table.acknowledge(1), Err(AlarmError::WrongState));
}

#[test]
fn clearing_ends_the_alarm_and_a_later_one_gets_a_new_id() {
    let mut table = AlarmTable::<4>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    assert_eq!(table.clear(AlarmKind::CarFault, Some(0)), Some(1));
    assert_eq!(table.get(1).map(|a| a.state), Some(AlarmState::Cleared));
    assert_eq!(table.clear(AlarmKind::CarFault, Some(0)), None);
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(0), 7),
        Ok(Raised::New(2))
    );
}

#[test]
fn clearing_finds_an_acknowledged_alarm_and_ignores_other_cars() {
    let mut table = AlarmTable::<4>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    table.raise(AlarmKind::CarFault, Some(1), 1).ok();
    table.acknowledge(1).ok();
    assert_eq!(table.clear(AlarmKind::CarFault, Some(0)), Some(1));
    assert_eq!(table.get(2).map(|a| a.state), Some(AlarmState::Active));
}

#[test]
fn a_full_table_evicts_a_cleared_alarm_but_never_an_active_one() {
    let mut table = AlarmTable::<2>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    table.raise(AlarmKind::CarFault, Some(1), 1).ok();
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(2), 1),
        Err(AlarmError::TableFull)
    );
    table.clear(AlarmKind::CarFault, Some(0));
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(2), 2),
        Ok(Raised::New(3))
    );
    assert_eq!(table.get(1), None);
    assert!(table.get(2).is_some());
    assert_eq!(table.total(), 2);
}

#[test]
fn an_acknowledged_alarm_is_not_evicted_either() {
    let mut table = AlarmTable::<1>::new();
    table.raise(AlarmKind::LogOverflow, None, 1).ok();
    table.acknowledge(1).ok();
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(0), 2),
        Err(AlarmError::TableFull)
    );
    assert!(table.get(1).is_some());
}

#[test]
fn pages_list_alarms_in_id_order_with_total_and_offset() {
    let mut table = AlarmTable::<16>::new();
    for car in 0..10 {
        table
            .raise(AlarmKind::CarFault, Some(car), u64::from(car))
            .ok();
    }
    let first = table.page(0);
    assert_eq!((first.total, first.offset), (10, 0));
    assert_eq!(ids(&first), (1..=8).collect::<Vec<_>>());
    let second = table.page(8);
    assert_eq!((second.total, second.offset), (10, 8));
    assert_eq!(ids(&second), vec![9, 10]);
    assert!(valid(first) && valid(second));
}

#[test]
fn a_page_beyond_the_end_is_empty_and_consistent() {
    let mut table = AlarmTable::<4>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    let page = table.page(50);
    assert_eq!((page.total, page.offset), (1, 1));
    assert_eq!(ids(&page), Vec::<u32>::new());
    assert!(valid(page));
    let empty = AlarmTable::<4>::new().page(0);
    assert_eq!((empty.total, empty.offset), (0, 0));
    assert!(valid(empty));
}

#[test]
fn ids_survive_eviction_and_stay_ordered_on_pages() {
    let mut table = AlarmTable::<2>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    table.raise(AlarmKind::CarFault, Some(1), 1).ok();
    table.clear(AlarmKind::CarFault, Some(0));
    table.raise(AlarmKind::CarFault, Some(2), 2).ok();
    assert_eq!(ids(&table.page(0)), vec![2, 3]);
}

#[test]
fn eviction_takes_the_oldest_cleared_alarm_first() {
    let mut table = AlarmTable::<2>::new();
    table.raise(AlarmKind::CarFault, Some(0), 1).ok();
    table.raise(AlarmKind::CarFault, Some(1), 1).ok();
    table.clear(AlarmKind::CarFault, Some(0));
    table.clear(AlarmKind::CarFault, Some(1));
    assert_eq!(
        table.raise(AlarmKind::CarFault, Some(2), 2),
        Ok(Raised::New(3))
    );
    assert_eq!(table.get(1), None);
    assert!(table.get(2).is_some());
}
