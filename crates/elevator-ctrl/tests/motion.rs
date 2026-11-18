use elevator_ctrl::motion::MotionParams;

fn params() -> MotionParams {
    MotionParams::new(1600, 1000, 1000).expect("параметры не нулевые")
}

#[test]
fn zero_speed_acceleration_or_jerk_is_rejected() {
    assert_eq!(MotionParams::new(0, 1000, 1000), None);
    assert_eq!(MotionParams::new(1600, 0, 1000), None);
    assert_eq!(MotionParams::new(1600, 1000, 0), None);
    assert!(MotionParams::new(1, 1, 1).is_some());
}

#[test]
fn braking_distance_covers_three_phase_and_two_phase_ramps() {
    let p = params();
    assert_eq!(p.braking_distance_mm(0), 0);
    assert_eq!(p.braking_distance_mm(1600), 2080);
    assert_eq!(p.braking_distance_mm(1000), 1000);
    assert_eq!(p.braking_distance_mm(500), 354);
    assert_eq!(p.braking_distance_mm(1), 1);
}

#[test]
fn stopping_distance_of_an_accelerating_car_includes_the_jerk_down_phase() {
    let p = params();
    assert_eq!(p.stopping_distance_mm(1000, 0), Some(1000));
    assert_eq!(p.stopping_distance_mm(500, 1000), Some(1834));
    assert_eq!(p.stopping_distance_mm(500, 1001), None);
    assert_eq!(p.stopping_distance_mm(1500, 1000), None);
}

#[test]
fn time_for_an_accelerating_car_is_the_settle_time_plus_the_stop() {
    let ms = params()
        .time_to_stop_from_ms(1834, 500, 1000)
        .expect("достижимо");
    assert!((3000..=3010).contains(&ms), "{ms}");
    assert_eq!(params().time_to_stop_from_ms(1833, 500, 1000), None);
    assert_eq!(
        params().time_to_stop_from_ms(5000, 800, 0),
        params().time_to_stop_ms(5000, 800)
    );
}

#[test]
fn standing_still_at_the_target_takes_no_time() {
    assert_eq!(params().time_to_stop_ms(0, 0), Some(0));
}

#[test]
fn from_rest_the_ramps_are_exact_and_the_rest_is_cruise() {
    let p = params();
    assert_eq!(p.time_to_stop_ms(4160, 0), Some(5200));
    assert_eq!(p.time_to_stop_ms(5760, 0), Some(6200));
    assert_eq!(p.time_to_stop_ms(27_000, 0), Some(19_475));
}

#[test]
fn from_rest_over_one_floor_the_peak_speed_is_below_the_maximum() {
    let ms = params().time_to_stop_ms(3000, 0).expect("достижимо");
    assert!((4606..=4609).contains(&ms), "{ms}");
}

#[test]
fn at_full_speed_exactly_the_braking_distance_takes_the_braking_time() {
    assert_eq!(params().time_to_stop_ms(2080, 1600), Some(2600));
}

#[test]
fn at_full_speed_with_extra_distance_cruises_first() {
    assert_eq!(params().time_to_stop_ms(3680, 1600), Some(3600));
}

#[test]
fn cannot_stop_at_the_target_if_braking_needs_more_road() {
    let p = params();
    assert_eq!(p.time_to_stop_ms(2079, 1600), None);
    assert_eq!(p.time_to_stop_ms(999, 1000), None);
    assert!(p.time_to_stop_ms(1000, 1000).is_some());
    assert_eq!(p.time_to_stop_ms(0, 1), None);
}

#[test]
fn speed_above_the_maximum_is_rejected() {
    assert_eq!(params().time_to_stop_ms(100_000, 1601), None);
}

#[test]
fn huge_distance_does_not_overflow() {
    let slow = MotionParams::new(1, 1, 1).expect("не нулевые");
    assert_eq!(slow.time_to_stop_ms(u32::MAX, 0), Some(u32::MAX));
}

struct Rng(u64);

impl Rng {
    fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn range(&mut self, low: u32, high: u32) -> u32 {
        let span = u64::from(high - low) + 1;
        low + u32::try_from(self.step() % span).expect("остаток меньше span")
    }
}

fn ramp(v1: f64, v2: f64, accel: f64, jerk: f64) -> (f64, f64) {
    let delta = v2 - v1;
    let seconds = if delta * jerk >= accel * accel {
        delta / accel + accel / jerk
    } else {
        2.0 * (delta / jerk).sqrt()
    };
    (seconds, f64::midpoint(v1, v2) * seconds)
}

fn ideal_ms(max_speed: u32, accel: u32, jerk: u32, distance: u32, speed: u32) -> f64 {
    ideal_ms_from(
        max_speed,
        accel,
        jerk,
        f64::from(distance),
        f64::from(speed),
    )
}

fn ideal_ms_from(max_speed: u32, accel: u32, jerk: u32, distance: f64, speed: f64) -> f64 {
    let (vmax, a, j) = (f64::from(max_speed), f64::from(accel), f64::from(jerk));
    let covered = |vc: f64| ramp(speed, vc, a, j).1 + ramp(0.0, vc, a, j).1;
    let cruise_speed = if covered(vmax) <= distance {
        vmax
    } else {
        let (mut low, mut high) = (speed, vmax);
        for _ in 0..200 {
            let mid = f64::midpoint(low, high);
            if covered(mid) <= distance {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    };
    let ramps = ramp(speed, cruise_speed, a, j).0 + ramp(0.0, cruise_speed, a, j).0;
    let cruise = if cruise_speed > 0.0 {
        (distance - covered(cruise_speed)).max(0.0) / cruise_speed
    } else {
        0.0
    };
    (ramps + cruise) * 1000.0
}

#[test]
fn random_cases_stay_within_a_few_milliseconds_above_the_ideal_time() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..20_000 {
        let max_speed = rng.range(200, 6000);
        let accel = rng.range(100, 3000);
        let jerk = rng.range(100, 5000);
        let distance = rng.range(1, 60_000);
        let speed = rng.range(0, max_speed);
        let p = MotionParams::new(max_speed, accel, jerk).expect("не нулевые");

        let reachable = distance >= p.braking_distance_mm(speed);
        let got = p.time_to_stop_ms(distance, speed);
        let case = format!("vmax={max_speed} a={accel} j={jerk} d={distance} v0={speed}");
        assert_eq!(got.is_some(), reachable, "{case}");
        if let Some(ms) = got {
            let ideal = ideal_ms(max_speed, accel, jerk, distance, speed);
            let ms = f64::from(ms);
            assert!(
                ms >= ideal - 1e-3 && ms <= ideal + 4.5,
                "{case}: получили {ms}, идеально {ideal}"
            );
        }
    }
}

#[test]
fn longer_distance_takes_no_less_time_up_to_one_millisecond() {
    let mut rng = Rng(42);
    for _ in 0..2000 {
        let max_speed = rng.range(200, 6000);
        let accel = rng.range(100, 3000);
        let jerk = rng.range(100, 5000);
        let speed = rng.range(0, max_speed);
        let p = MotionParams::new(max_speed, accel, jerk).expect("не нулевые");
        let start = p.braking_distance_mm(speed);
        let mut last = p
            .time_to_stop_ms(start, speed)
            .expect("тормозной путь достижим");
        for step in 1..=50 {
            let now = p
                .time_to_stop_ms(start + step * 997, speed)
                .expect("дальше тормозного пути цель достижима");
            assert!(
                now + 1 >= last,
                "время уменьшилось при росте пути: {last} -> {now}"
            );
            last = now;
        }
    }
}

fn ideal_ms_accelerating(
    max_speed: u32,
    accel: u32,
    jerk: u32,
    distance: u32,
    speed: u32,
    a0: u32,
) -> f64 {
    let (j, v0, a0) = (f64::from(jerk), f64::from(speed), f64::from(a0));
    let settle_s = a0 / j;
    let path = v0 * settle_s + a0 * settle_s * settle_s / 2.0 - j * settle_s.powi(3) / 6.0;
    let v1 = v0 + a0 * a0 / (2.0 * j);
    settle_s * 1000.0 + ideal_ms_from(max_speed, accel, jerk, f64::from(distance) - path, v1)
}

#[test]
fn random_accelerating_cases_stay_close_to_the_ideal_plan() {
    let mut rng = Rng(0xDEAD_BEEF_1234_5678);
    for _ in 0..20_000 {
        let max_speed = rng.range(400, 6000);
        let accel = rng.range(200, 3000);
        let jerk = rng.range(200, 5000);
        let speed = rng.range(0, max_speed);
        let a0 = rng.range(0, accel);
        let distance = rng.range(1, 60_000);
        let p = MotionParams::new(max_speed, accel, jerk).expect("не нулевые");

        let case = format!("vmax={max_speed} a={accel} j={jerk} d={distance} v0={speed} a0={a0}");
        let stopping = p.stopping_distance_mm(speed, a0);
        let got = p.time_to_stop_from_ms(distance, speed, a0);
        assert_eq!(
            got.is_some(),
            stopping.is_some_and(|s| distance >= s),
            "{case}"
        );
        if let Some(ms) = got {
            let ideal = ideal_ms_accelerating(max_speed, accel, jerk, distance, speed, a0);
            let ms = f64::from(ms);
            assert!(
                ms >= ideal - 1e-3 && ms <= ideal + 6.0,
                "{case}: получили {ms}, идеально {ideal}"
            );
        }
    }
}
