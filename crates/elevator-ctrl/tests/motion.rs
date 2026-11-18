//! Тесты кинематики кабины.
//!
//! Примерные параметры из README: скорость 3 м/с и ускорение 1,5 м/с², то есть
//! 3000 мм/с и 1500 мм/с². Тормозной путь с полной скорости 3000 мм (ровно один этаж),
//! время разгона до полной скорости 2 с.

use elevator_ctrl::motion::MotionParams;

fn params() -> MotionParams {
    MotionParams::new(3000, 1500).expect("параметры не нулевые")
}

#[test]
fn zero_speed_or_acceleration_is_rejected() {
    assert_eq!(MotionParams::new(0, 1500), None);
    assert_eq!(MotionParams::new(3000, 0), None);
    assert_eq!(MotionParams::new(0, 0), None);
    assert!(MotionParams::new(1, 1).is_some());
}

#[test]
fn braking_distance_is_v_squared_over_two_a_rounded_up() {
    let p = params();
    assert_eq!(p.braking_distance_mm(0), 0);
    assert_eq!(p.braking_distance_mm(3000), 3000);
    assert_eq!(p.braking_distance_mm(1500), 750);
    // 1000² / 3000 = 333,33: округляем вверх.
    assert_eq!(p.braking_distance_mm(1000), 334);
    assert_eq!(p.braking_distance_mm(1), 1);
}

#[test]
fn standing_still_at_the_target_takes_no_time() {
    assert_eq!(params().time_to_stop_ms(0, 0), Some(0));
}

#[test]
fn from_rest_over_nine_floors_is_a_trapezoid() {
    // 27 000 мм: разгон 3000 мм за 2 с и торможение 3000 мм за 2 с, остальные 21 000 мм
    // хода по 3000 мм/с это 7 с. Итого 11 с.
    assert_eq!(params().time_to_stop_ms(27_000, 0), Some(11_000));
}

#[test]
fn from_rest_three_floors_is_a_trapezoid() {
    // 9000 мм: разгон 3000 мм за 2 с, ход 3000 мм за 1 с, торможение 3000 мм за 2 с.
    assert_eq!(params().time_to_stop_ms(9000, 0), Some(5000));
}

#[test]
fn from_rest_exactly_at_the_trapezoid_boundary() {
    // 6000 мм: разгон и торможение без хода: 2 + 2 с.
    assert_eq!(params().time_to_stop_ms(6000, 0), Some(4000));
}

#[test]
fn from_rest_one_floor_is_a_triangle_rounded_up() {
    // Пик скорости не достигается: t = 2 * sqrt(3000 / 1500) = 2,8284 с, вверх 2829 мс.
    assert_eq!(params().time_to_stop_ms(3000, 0), Some(2829));
}

#[test]
fn from_rest_one_millimetre_is_rounded_up_to_whole_milliseconds() {
    // 2 * sqrt(1 / 1500) = 51,64 мс.
    assert_eq!(params().time_to_stop_ms(1, 0), Some(52));
}

#[test]
fn at_full_speed_exactly_the_braking_distance_takes_the_braking_time() {
    assert_eq!(params().time_to_stop_ms(3000, 3000), Some(2000));
}

#[test]
fn at_full_speed_with_extra_distance_cruises_first() {
    // 3000 мм хода за 1 с и торможение 2 с.
    assert_eq!(params().time_to_stop_ms(6000, 3000), Some(3000));
}

#[test]
fn at_half_speed_accelerates_cruises_and_brakes() {
    // Разгон 1500 до 3000 за 1 с (2250 мм), торможение 2 с (3000 мм), ход 3750 мм за 1,25 с.
    assert_eq!(params().time_to_stop_ms(9000, 1500), Some(4250));
}

#[test]
fn at_half_speed_on_a_short_way_the_peak_is_below_the_maximum() {
    // Пик sqrt(1500 * 3000 + 1500² / 2) = 2371,7 мм/с, t = (2 * 2371,7 - 1500) / 1500 = 2,1623 с.
    assert_eq!(params().time_to_stop_ms(3000, 1500), Some(2163));
}

#[test]
fn cannot_stop_at_the_target_if_braking_needs_more_road() {
    let p = params();
    assert_eq!(p.time_to_stop_ms(2999, 3000), None);
    assert_eq!(p.time_to_stop_ms(749, 1500), None);
    assert_eq!(p.time_to_stop_ms(750, 1500), Some(1000));
    assert_eq!(p.time_to_stop_ms(0, 1), None);
}

#[test]
fn speed_above_the_maximum_is_rejected() {
    assert_eq!(params().time_to_stop_ms(100_000, 3001), None);
}

#[test]
fn huge_distance_does_not_overflow() {
    // 1 мм/с и 1 мм/с²: время огромное, обрезается до u32::MAX, без паники.
    let slow = MotionParams::new(1, 1).expect("не нулевые");
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

// Независимый эталон на f64: время в миллисекундах без округления.
fn exact_ms(max_speed: u32, accel: u32, distance: u32, speed: u32) -> f64 {
    let (vmax, a, d, v0) = (
        f64::from(max_speed),
        f64::from(accel),
        f64::from(distance),
        f64::from(speed),
    );
    let trapezoid_distance = (2.0 * vmax * vmax - v0 * v0) / (2.0 * a);
    let seconds = if d >= trapezoid_distance {
        (2.0 * vmax - v0) / a + (d - trapezoid_distance) / vmax
    } else {
        let peak = (a * d + v0 * v0 / 2.0).sqrt();
        (2.0 * peak - v0) / a
    };
    seconds * 1000.0
}

#[test]
fn random_cases_are_the_smallest_whole_milliseconds_not_below_the_exact_time() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..20_000 {
        let max_speed = rng.range(200, 6000);
        let accel = rng.range(100, 4000);
        let distance = rng.range(0, 60_000);
        let speed = rng.range(0, max_speed);
        let p = MotionParams::new(max_speed, accel).expect("не нулевые");

        let reachable = distance >= p.braking_distance_mm(speed);
        let got = p.time_to_stop_ms(distance, speed);
        assert_eq!(
            got.is_some(),
            reachable,
            "vmax={max_speed} a={accel} d={distance} v0={speed}"
        );
        if let Some(ms) = got {
            let exact = exact_ms(max_speed, accel, distance, speed);
            let ms = f64::from(ms);
            assert!(
                ms >= exact - 1e-6 && ms < exact + 1.0 + 1e-6,
                "vmax={max_speed} a={accel} d={distance} v0={speed}: получили {ms}, точно {exact}"
            );
        }
    }
}

#[test]
fn longer_distance_never_takes_less_time() {
    let mut rng = Rng(42);
    for _ in 0..2000 {
        let max_speed = rng.range(200, 6000);
        let accel = rng.range(100, 4000);
        let speed = rng.range(0, max_speed);
        let p = MotionParams::new(max_speed, accel).expect("не нулевые");
        let start = p.braking_distance_mm(speed);
        let mut last = p
            .time_to_stop_ms(start, speed)
            .expect("тормозной путь достижим");
        for step in 1..=50 {
            let now = p
                .time_to_stop_ms(start + step * 997, speed)
                .expect("дальше тормозного пути цель достижима");
            assert!(now >= last, "время уменьшилось при росте пути");
            last = now;
        }
    }
}
