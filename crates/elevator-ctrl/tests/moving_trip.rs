use elevator_ctrl::motion::{MotionParams, MovingTrip};

fn lift() -> MotionParams {
    MotionParams::new(1600, 1000, 1000).unwrap()
}

fn starts() -> Vec<(u32, u32)> {
    let mut all = Vec::new();
    for speed in [0, 1, 37, 300, 800, 1100, 1599, 1600] {
        for accel in [0, 1, 250, 700, 1000] {
            let real = u64::from(speed) * 2000 >= u64::from(accel) * u64::from(accel);
            if real && lift().stopping_distance_mm(speed, accel).is_some() {
                all.push((speed, accel));
            }
        }
    }
    all
}

fn trips() -> Vec<(u32, u32, u32, MovingTrip)> {
    let mut all = Vec::new();
    for (speed, accel) in starts() {
        let stop = lift().stopping_distance_mm(speed, accel).unwrap();
        for distance in [stop, stop + 1, stop + 999, stop + 6000, 27000.max(stop)] {
            let trip = lift().moving_trip(distance, speed, accel).unwrap();
            all.push((distance, speed, accel, trip));
        }
    }
    all
}

#[test]
fn braking_from_full_speed_follows_the_jerk_limited_curve() {
    let trip = lift().moving_trip(2080, 1600, 0).unwrap();
    assert_eq!(Some(trip.duration_ms()), lift().time_to_stop_ms(2080, 1600));
    assert_eq!(trip.duration_ms(), 2600);
    for (t, sample) in [
        (0, (0, 1600, 0)),
        (1000, (1433, 1100, -1000)),
        (1300, (1718, 800, -1000)),
        (2600, (2080, 0, 0)),
        (9999, (2080, 0, 0)),
    ] {
        assert_eq!(trip.sample(t), sample, "t={t}");
    }
}

#[test]
fn the_trip_lasts_exactly_as_long_as_the_eta_says() {
    for (distance, speed, accel, trip) in trips() {
        assert_eq!(
            Some(trip.duration_ms()),
            lift().time_to_stop_from_ms(distance, speed, accel),
            "d={distance} v={speed} a={accel}"
        );
        assert_eq!(trip.sample(trip.duration_ms()), (distance, 0, 0));
        assert_eq!(trip.sample(u32::MAX), (distance, 0, 0));
    }
}

#[test]
fn the_trip_starts_from_the_given_speed_and_acceleration() {
    for (distance, speed, accel, trip) in trips() {
        let (travelled, v, a) = trip.sample(0);
        let case = format!("d={distance} v={speed} a={accel}: got v={v} a={a}");
        assert_eq!(travelled, 0, "{case}");
        assert!(v.abs_diff(speed) <= 1 + speed / 200, "{case}");
        let accel = u64::from(accel);
        assert!(
            a.unsigned_abs().abs_diff(accel) <= 1 + accel / 100,
            "{case}"
        );
        assert!(a >= 0, "{case}");
    }
}

#[test]
fn every_millisecond_moves_smoothly_within_the_limits() {
    for (distance, speed, accel, trip) in trips() {
        let case = format!("d={distance} v={speed} a={accel}");
        let mut previous = trip.sample(0);
        for t in 1..=trip.duration_ms() {
            let now = trip.sample(t);
            assert!(
                now.0 >= previous.0 && now.0 - previous.0 <= 2,
                "{case} t={t}"
            );
            assert!(now.1 <= 1600, "{case} t={t} v={}", now.1);
            assert!(now.1.abs_diff(previous.1) <= 2, "{case} t={t}");
            assert!(now.2.unsigned_abs() <= 1010, "{case} t={t} a={}", now.2);
            assert!(now.2.abs_diff(previous.2) <= 12, "{case} t={t}");
            previous = now;
        }
    }
}

#[test]
fn reported_speed_and_acceleration_agree_with_the_path() {
    for (distance, speed, accel, trip) in trips() {
        let case = format!("d={distance} v={speed} a={accel}");
        let end = trip.duration_ms();
        for t in (0..end.saturating_sub(20)).step_by(7) {
            let (x0, v0, a0) = trip.sample(t);
            let (x1, v1, a1) = trip.sample(t + 20);
            let mean_speed = (u64::from(v0) + u64::from(v1)) / 100;
            assert!(u64::from(x1 - x0).abs_diff(mean_speed) <= 2, "{case} t={t}");
            let gained = i64::from(v1) - i64::from(v0);
            let mean_accel = (a0 + a1) / 100;
            assert!((gained - mean_accel).abs() <= 2, "{case} t={t}");
        }
    }
}

#[test]
fn a_stop_closer_than_the_stopping_distance_is_refused() {
    for (speed, accel) in starts() {
        let stop = lift().stopping_distance_mm(speed, accel).unwrap();
        if stop > 0 {
            assert!(lift().moving_trip(stop - 1, speed, accel).is_none());
        }
        assert!(lift().moving_trip(stop, speed, accel).is_some());
    }
    assert!(lift().moving_trip(27000, 1601, 0).is_none());
    assert!(lift().moving_trip(27000, 0, 1001).is_none());
    assert!(lift().moving_trip(27000, 1500, 1000).is_none());
}

#[test]
fn replanning_while_cruising_keeps_the_arrival_and_the_path() {
    let whole = lift().moving_trip(27000, 0, 0).unwrap();
    for at in [2600, 3000, 7777, 14000] {
        let (done, speed, accel) = whole.sample(at);
        assert_eq!((speed, accel), (1600, 0), "at={at}");
        let rest = lift().moving_trip(27000 - done, speed, 0).unwrap();
        let left = whole.duration_ms() - at;
        assert!(rest.duration_ms().abs_diff(left) <= 3, "at={at}");
        for t in (0..=left).step_by(10) {
            let expected = whole.sample(at + t).0;
            let replanned = done + rest.sample(t).0;
            assert!(expected.abs_diff(replanned) <= 3, "at={at} t={t}");
        }
    }
}

#[test]
fn extreme_parameters_neither_panic_nor_break_the_endpoints() {
    for params in [
        MotionParams::new(100, 1, 7).unwrap(),
        MotionParams::new(u32::MAX, u32::MAX, u32::MAX).unwrap(),
        MotionParams::new(1, 1, 1).unwrap(),
    ] {
        for (speed, accel) in [(0, 0), (1, 0), (0, 1), (100, 0), (u32::MAX, 0)] {
            for distance in [0, 1, 17, 4321, 27000, u32::MAX] {
                let eta = params.time_to_stop_from_ms(distance, speed, accel);
                let Some(trip) = params.moving_trip(distance, speed, accel) else {
                    assert!(eta.is_none_or(|eta| eta == u32::MAX));
                    continue;
                };
                assert_eq!(Some(trip.duration_ms()), eta);
                assert_eq!(trip.sample(0).0, 0);
                assert_eq!(trip.sample(trip.duration_ms()), (distance, 0, 0));
                let mut previous = 0;
                for i in 0..=50u64 {
                    let t = u32::try_from(u64::from(trip.duration_ms()) * i / 50).unwrap();
                    let (travelled, v, _) = trip.sample(t);
                    assert!((previous..=distance).contains(&travelled));
                    assert!(v <= params.max_speed_mm_s());
                    previous = travelled;
                }
            }
        }
    }
}

#[test]
fn replanning_while_accelerating_keeps_the_arrival_and_the_path() {
    let whole = lift().moving_trip(27000, 0, 0).unwrap();
    for at in [300, 700, 1000, 1400, 1800, 2200] {
        let (done, speed, accel) = whole.sample(at);
        let accel = u32::try_from(accel).unwrap();
        let rest = lift().moving_trip(27000 - done, speed, accel).unwrap();
        let left = whole.duration_ms() - at;
        assert!(
            rest.duration_ms().abs_diff(left) <= 12,
            "at={at}: {} vs {left}",
            rest.duration_ms()
        );
        for t in (0..=left).step_by(10) {
            let expected = whole.sample(at + t).0;
            let replanned = done + rest.sample(t).0;
            assert!(expected.abs_diff(replanned) <= 20, "at={at} t={t}");
        }
    }
}

#[test]
fn a_replan_on_the_rising_edge_keeps_accelerating_up_to_the_peak() {
    let whole = lift().moving_trip(27000, 0, 0).unwrap();
    for at in [200, 500, 800] {
        let (done, speed, accel) = whole.sample(at);
        let accel = u32::try_from(accel).unwrap();
        let rest = lift().moving_trip(27000 - done, speed, accel).unwrap();
        let mut previous = rest.sample(0).2;
        for t in 1..=(1000 - accel) {
            let now = rest.sample(t).2;
            assert!(now + 2 >= previous, "at={at} t={t}: {previous} -> {now}");
            previous = now;
        }
        assert!(previous >= 990, "at={at}: peak {previous}");
    }
}
