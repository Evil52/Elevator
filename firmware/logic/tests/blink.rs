use core::num::NonZeroU32;
use elevator_fw_logic::led_on;

fn period(ms: u32) -> NonZeroU32 {
    NonZeroU32::new(ms).unwrap_or(NonZeroU32::MIN)
}

#[test]
fn led_is_on_in_the_first_half_of_the_period() {
    for now in [0, 1, 250, 499] {
        assert!(led_on(now, period(1000)), "now = {now}");
    }
}

#[test]
fn led_is_off_in_the_second_half_of_the_period() {
    for now in [500, 501, 750, 999] {
        assert!(!led_on(now, period(1000)), "now = {now}");
    }
}

#[test]
fn pattern_repeats_every_period() {
    for cycle in [1_u64, 2, 59, 3_600, 86_400_000] {
        let base = cycle * 1000;
        assert!(led_on(base, period(1000)), "base = {base}");
        assert!(led_on(base + 499, period(1000)), "base = {base}");
        assert!(!led_on(base + 500, period(1000)), "base = {base}");
        assert!(!led_on(base + 999, period(1000)), "base = {base}");
    }
}

#[test]
fn one_second_period_blinks_once_per_second() {
    let mut rising_edges = 0;
    let mut previous = led_on(0, period(1000));
    for now in 1..=10_000 {
        let current = led_on(now, period(1000));
        if current && !previous {
            rising_edges += 1;
        }
        previous = current;
    }
    assert_eq!(rising_edges, 10);
}

#[test]
fn odd_period_keeps_the_longer_part_on() {
    assert!(led_on(0, period(3)));
    assert!(led_on(1, period(3)));
    assert!(!led_on(2, period(3)));
    assert!(led_on(3, period(3)));
}

#[test]
fn period_of_one_millisecond_is_always_on() {
    for now in [0, 1, 2, 1_000_000] {
        assert!(led_on(now, period(1)), "now = {now}");
    }
}

#[test]
fn extreme_times_do_not_overflow() {
    assert!(led_on(u64::MAX, period(u32::MAX)));
    assert!(!led_on(u64::MAX, period(1000)));
    assert!(led_on(u64::from(u32::MAX), period(u32::MAX)));
    assert!(!led_on(u64::from(u32::MAX) - 1, period(u32::MAX)));
}
