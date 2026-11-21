use elevator_fw_logic::button::{Button, ButtonEvent, DEBOUNCE_MS, LONG_PRESS_MS};

fn run(button: &mut Button, from: u64, to: u64, pressed: bool) -> Vec<(u64, ButtonEvent)> {
    (from..=to)
        .filter_map(|now| button.update(now, pressed).map(|event| (now, event)))
        .collect()
}

#[test]
fn idle_button_gives_no_events() {
    let mut button = Button::new();
    assert_eq!(run(&mut button, 0, 5_000, false), Vec::new());
}

#[test]
fn a_normal_press_gives_one_short_event_on_release() {
    let mut button = Button::new();
    assert_eq!(run(&mut button, 100, 299, true), Vec::new());
    assert_eq!(
        run(&mut button, 300, 400, false),
        vec![(300 + DEBOUNCE_MS, ButtonEvent::Short)]
    );
}

#[test]
fn a_glitch_shorter_than_the_debounce_time_is_ignored() {
    let mut button = Button::new();
    assert_eq!(
        run(&mut button, 100, 100 + DEBOUNCE_MS - 2, true),
        Vec::new()
    );
    assert_eq!(
        run(&mut button, 100 + DEBOUNCE_MS - 1, 500, false),
        Vec::new()
    );
}

#[test]
fn contact_bounce_while_pressing_gives_a_single_short_event() {
    let mut button = Button::new();
    let mut events = Vec::new();
    for (now, pressed) in [
        (100, true),
        (103, false),
        (106, true),
        (109, false),
        (112, true),
    ] {
        events.extend(button.update(now, pressed));
    }
    events.extend(run(&mut button, 113, 400, true).into_iter().map(|(_, e)| e));
    events.extend(
        run(&mut button, 401, 500, false)
            .into_iter()
            .map(|(_, e)| e),
    );
    assert_eq!(events, vec![ButtonEvent::Short]);
}

#[test]
fn bounce_while_releasing_gives_a_single_short_event() {
    let mut button = Button::new();
    assert_eq!(run(&mut button, 0, 300, true), Vec::new());
    let mut events = Vec::new();
    for (now, pressed) in [(301, false), (304, true), (307, false)] {
        events.extend(button.update(now, pressed));
    }
    events.extend(
        run(&mut button, 308, 400, false)
            .into_iter()
            .map(|(_, e)| e),
    );
    assert_eq!(events, vec![ButtonEvent::Short]);
}

#[test]
fn a_release_glitch_during_a_hold_does_not_end_the_press() {
    let mut button = Button::new();
    assert_eq!(run(&mut button, 0, 400, true), Vec::new());
    assert!(button.update(401, false).is_none());
    assert!(button.update(405, true).is_none());
    assert_eq!(run(&mut button, 406, 1_000, true), Vec::new());
    assert_eq!(
        run(&mut button, 1_001, 1_100, false),
        vec![(1_001 + DEBOUNCE_MS, ButtonEvent::Short)]
    );
}

#[test]
fn a_hold_gives_one_long_event_at_the_threshold_and_no_short_event() {
    let mut button = Button::new();
    let held = run(&mut button, 0, 5_000, true);
    assert_eq!(held, vec![(LONG_PRESS_MS, ButtonEvent::Long)]);
    assert_eq!(run(&mut button, 5_001, 5_200, false), Vec::new());
}

#[test]
fn a_release_confirmed_at_the_threshold_but_started_before_it_is_short() {
    let mut button = Button::new();
    assert_eq!(
        run(&mut button, 0, LONG_PRESS_MS - DEBOUNCE_MS - 1, true),
        Vec::new()
    );
    let after = run(&mut button, LONG_PRESS_MS - DEBOUNCE_MS, 3_000, false);
    assert_eq!(after, vec![(LONG_PRESS_MS, ButtonEvent::Short)]);
}

#[test]
fn consecutive_presses_each_give_their_own_event() {
    let mut button = Button::new();
    let mut events = Vec::new();
    events.extend(run(&mut button, 0, 99, true));
    events.extend(run(&mut button, 100, 199, false));
    events.extend(run(&mut button, 200, 299, true));
    events.extend(run(&mut button, 300, 399, false));
    assert_eq!(
        events
            .into_iter()
            .map(|(_, event)| event)
            .collect::<Vec<_>>(),
        vec![ButtonEvent::Short, ButtonEvent::Short]
    );
}

#[test]
fn a_short_press_after_a_long_one_still_works() {
    let mut button = Button::new();
    run(&mut button, 0, 2_000, true);
    run(&mut button, 2_001, 2_100, false);
    assert_eq!(run(&mut button, 2_101, 2_200, true), Vec::new());
    let events = run(&mut button, 2_201, 2_300, false);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, ButtonEvent::Short);
}

#[test]
fn times_near_the_top_of_the_range_do_not_overflow() {
    let mut button = Button::new();
    let start = u64::MAX - 5_000;
    let events: Vec<_> = (0..4_000_u64)
        .filter_map(|offset| button.update(start + offset, true))
        .collect();
    assert_eq!(events, vec![ButtonEvent::Long]);
}
