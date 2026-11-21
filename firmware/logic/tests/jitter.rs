use core::num::NonZeroU32;
use elevator_fw_logic::{JitterReport, JitterStats};

fn stats(period_us: u32) -> JitterStats {
    JitterStats::new(NonZeroU32::new(period_us).unwrap_or(NonZeroU32::MIN))
}

fn report(count: u32, min_us: u32, mean_us: u32, max_us: u32, overruns: u32) -> JitterReport {
    JitterReport {
        count,
        min_us,
        mean_us,
        max_us,
        overruns,
    }
}

#[test]
fn nothing_recorded_gives_no_report() {
    assert_eq!(stats(10_000).take(), None);
}

#[test]
fn single_sample_is_min_mean_and_max() {
    let mut s = stats(10_000);
    s.record(42);
    assert_eq!(s.take(), Some(report(1, 42, 42, 42, 0)));
}

#[test]
fn several_samples_give_min_max_and_floored_mean() {
    let mut s = stats(10_000);
    for late in [10, 40, 20] {
        s.record(late);
    }
    assert_eq!(s.take(), Some(report(3, 10, 23, 40, 0)));
}

#[test]
fn zero_lateness_is_a_valid_minimum() {
    let mut s = stats(10_000);
    s.record(7);
    s.record(0);
    assert_eq!(s.take(), Some(report(2, 0, 3, 7, 0)));
}

#[test]
fn take_starts_a_fresh_window() {
    let mut s = stats(10_000);
    s.record(5);
    s.record(900);
    assert!(s.take().is_some());
    assert_eq!(s.take(), None);
    s.record(300);
    assert_eq!(s.take(), Some(report(1, 300, 300, 300, 0)));
}

#[test]
fn lateness_of_a_full_period_is_an_overrun() {
    let mut s = stats(10_000);
    s.record(9_999);
    assert_eq!(s.take(), Some(report(1, 9_999, 9_999, 9_999, 0)));
    s.record(10_000);
    assert_eq!(s.take(), Some(report(1, 10_000, 10_000, 10_000, 1)));
}

#[test]
fn overruns_are_counted_per_sample_and_reset_with_the_window() {
    let mut s = stats(1_000);
    for late in [0, 1_000, 5_000, 999, u32::MAX] {
        s.record(late);
    }
    let first = s.take();
    assert_eq!(first.map(|r| (r.count, r.overruns)), Some((5, 3)));
    s.record(1);
    assert_eq!(s.take().map(|r| r.overruns), Some(0));
}

#[test]
fn sum_of_maximal_samples_does_not_overflow() {
    let mut s = stats(1);
    for _ in 0..3 {
        s.record(u32::MAX);
    }
    assert_eq!(s.take(), Some(report(3, u32::MAX, u32::MAX, u32::MAX, 3)));
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

#[test]
fn random_sequences_match_a_vec_model() {
    let mut rng = Lcg(0x5EED);
    for round in 0..2_000 {
        let period = (rng.next() % 5_000) + 1;
        let len = (rng.next() % 40) as usize;
        let samples: Vec<u32> = (0..len)
            .map(|_| match rng.next() % 4 {
                0 => rng.next() % 50,
                1 => rng.next() % 20_000,
                2 => rng.next(),
                _ => u32::MAX - rng.next() % 3,
            })
            .collect();
        let mut s = stats(period);
        for &late in &samples {
            s.record(late);
        }
        let expected = if samples.is_empty() {
            None
        } else {
            let sum: u64 = samples.iter().map(|&v| u64::from(v)).sum();
            Some(report(
                u32::try_from(samples.len()).unwrap_or(u32::MAX),
                *samples.iter().min().unwrap_or(&0),
                u32::try_from(sum / samples.len() as u64).unwrap_or(u32::MAX),
                *samples.iter().max().unwrap_or(&0),
                u32::try_from(samples.iter().filter(|&&v| v >= period).count()).unwrap_or(u32::MAX),
            ))
        };
        assert_eq!(
            s.take(),
            expected,
            "round {round}, period {period}, {samples:?}"
        );
    }
}
