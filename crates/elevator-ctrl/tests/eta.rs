//! Тесты расчёта ETA.
//!
//! Здание: 10 этажей по 3000 мм, этаж `n` лежит на `(n - 1) * 3000` мм. Движение: 1600 мм/с,
//! 1000 мм/с², рывок 1000 мм/с³. Двери: открытие 2000, выдержка 5000, закрытие 3000 мс,
//! цикл 10 000 мс. Время проезда в ожиданиях считается через `drive`, поэтому тесты
//! проверяют маршрут, а не саму кинематику.

use elevator_ctrl::eta::{
    Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams, QueuedCall, build_matrix,
    eta_ms,
};
use elevator_ctrl::motion::MotionParams;
use elevator_ctrl::{Cell, assign};

use Direction::{Down, Up};

const FLOORS: u32 = 10;
const HEIGHT: u32 = 3000;
const CYCLE: u32 = 11_000;

fn motion() -> MotionParams {
    MotionParams::new(1600, 1000, 1000).expect("движение")
}

fn params() -> EtaParams {
    EtaParams {
        building: Building::new(10, HEIGHT).expect("здание"),
        motion: motion(),
        doors: DoorTimes::new(2000, 6000, 3000).expect("двери"),
        aging_cap_ms: 0,
    }
}

// Время проезда между остановками с места.
fn drive(mm: u32) -> u32 {
    motion()
        .time_to_stop_ms(mm, 0)
        .expect("с места доехать можно")
}

// Время до остановки для кабины, которая уже идёт со скоростью `speed`.
fn drive_from(mm: u32, speed: u32) -> u32 {
    motion()
        .time_to_stop_ms(mm, speed)
        .expect("остановиться успевает")
}

const fn pos(floor: u32) -> u32 {
    (floor - 1) * HEIGHT
}

fn mask(floors: &[u32]) -> u32 {
    floors.iter().fold(0, |acc, floor| acc | (1 << (floor - 1)))
}

// Стоящая кабина на этаже без назначений и с закрытыми дверями.
fn idle(floor: u32, heading: Direction) -> CarState {
    CarState {
        position_mm: pos(floor),
        speed_mm_s: 0,
        accel_mm_s2: 0,
        heading,
        door_remaining_ms: 0,
        up_calls: 0,
        down_calls: 0,
        car_calls: 0,
        mode: CarMode::Normal,
    }
}

fn with_calls(car: CarState, ups: &[u32], downs: &[u32]) -> CarState {
    CarState {
        up_calls: mask(ups),
        down_calls: mask(downs),
        ..car
    }
}

fn with_car_calls(car: CarState, floors: &[u32]) -> CarState {
    CarState {
        car_calls: mask(floors),
        ..car
    }
}

// Кабина, идущая вверх на полной скорости на уровне 2-го этажа.
fn flying() -> CarState {
    CarState {
        position_mm: pos(2),
        speed_mm_s: 1600,
        ..idle(1, Up)
    }
}

fn call(floor: u32, direction: Direction) -> Call {
    Call {
        floor: u8::try_from(floor).expect("этаж"),
        direction,
    }
}

fn eta(car: &CarState, floor: u32, direction: Direction) -> Option<u32> {
    eta_ms(&params(), car, call(floor, direction))
}

fn queued(floor: u32, direction: Direction, waited_ms: u32) -> QueuedCall {
    QueuedCall {
        call: call(floor, direction),
        waited_ms,
    }
}

#[test]
fn idle_car_drives_straight_to_the_call() {
    assert_eq!(eta(&idle(1, Up), 3, Up), Some(drive(6000)));
    // Направление кнопки не важно, если у кабины нет других дел.
    assert_eq!(eta(&idle(1, Up), 3, Down), Some(drive(6000)));
}

#[test]
fn doors_still_closing_delay_the_start() {
    let car = CarState {
        door_remaining_ms: 1500,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 3, Up), Some(1500 + drive(6000)));
}

#[test]
fn call_on_the_floor_where_the_idle_car_stands_is_immediate() {
    assert_eq!(eta(&idle(5, Up), 5, Down), Some(0));
    let car = CarState {
        door_remaining_ms: 700,
        ..idle(5, Up)
    };
    assert_eq!(eta(&car, 5, Up), Some(700));
}

#[test]
fn idle_car_goes_down_to_a_call_below() {
    assert_eq!(eta(&idle(5, Up), 1, Down), Some(drive(12_000)));
    assert_eq!(eta(&idle(10, Down), 7, Down), Some(drive(9000)));
}

#[test]
fn unavailable_car_gets_no_estimate() {
    let car = CarState {
        mode: CarMode::Unavailable,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 3, Up), None);
}

#[test]
fn impossible_inputs_get_no_estimate() {
    let car = idle(1, Up);
    assert_eq!(eta(&car, 0, Up), None);
    assert_eq!(eta(&car, 11, Up), None);
    let outside = CarState {
        position_mm: pos(10) + 1,
        ..car
    };
    assert_eq!(eta(&outside, 5, Up), None);
    let too_fast = CarState {
        speed_mm_s: 1601,
        ..idle(1, Up)
    };
    assert_eq!(eta(&too_fast, 5, Up), None);
    for ghost in [
        CarState {
            up_calls: 1 << 10,
            ..car
        },
        CarState {
            down_calls: 1 << 31,
            ..car
        },
        CarState {
            car_calls: 1 << 10,
            ..car
        },
    ] {
        assert_eq!(eta(&ghost, 5, Up), None);
    }
}

#[test]
fn moving_car_reaches_a_call_ahead() {
    // Едет вверх на полной скорости с уровня 2-го этажа. Цель 6-й этаж: 12 000 мм.
    assert_eq!(eta(&flying(), 6, Up), Some(drive_from(12_000, 1600)));
    // 3-й этаж в 3000 мм: тормозной путь с этой скорости 2080 мм, успевает.
    assert_eq!(eta(&flying(), 3, Up), Some(drive_from(3000, 1600)));
}

#[test]
fn moving_car_ignores_the_door_timer() {
    let car = CarState {
        door_remaining_ms: 9999,
        ..flying()
    };
    assert_eq!(eta(&car, 6, Up), eta(&flying(), 6, Up));
}

#[test]
fn moving_car_cannot_stop_at_a_floor_it_is_already_passing() {
    // Кабина на уровне 2-го этажа, вызов вверх на 2-м. Ближайший этаж, где можно встать,
    // 3-й (дальше тормозного пути 2080 мм). Там разворот без дверей и возврат вниз.
    assert_eq!(
        eta(&flying(), 2, Up),
        Some(drive_from(3000, 1600) + drive(3000))
    );
}

#[test]
fn moving_car_past_the_last_reachable_floor_gets_no_estimate() {
    let car = CarState {
        position_mm: pos(9) + 2000,
        ..flying()
    };
    assert_eq!(eta(&car, 5, Down), None);
}

#[test]
fn on_the_way_call_is_not_delayed_by_a_later_stop() {
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 3, Up), Some(drive(6000)));
}

#[test]
fn on_the_way_call_beyond_a_planned_stop_waits_for_its_doors() {
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 7, Up), Some(drive(9000) + CYCLE + drive(9000)));
}

#[test]
fn opposite_call_is_taken_at_the_turnaround_point() {
    // Есть вызов вверх на 4-й, новый вызов вниз на 6-й: кабина поднимается до 6-го,
    // это точка разворота.
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 6, Down), Some(drive(9000) + CYCLE + drive(6000)));
}

#[test]
fn opposite_call_below_the_turnaround_is_taken_on_the_way_back() {
    // Подъём на 8-й, потом спуск на 5-й.
    let car = with_calls(idle(1, Up), &[8], &[]);
    assert_eq!(
        eta(&car, 5, Down),
        Some(drive(21_000) + CYCLE + drive(9000))
    );
}

#[test]
fn call_in_the_same_direction_behind_waits_for_the_next_sweep() {
    // Едет вверх с 5-го этажа к 8-му, вызов вверх на 3-м остался позади: сначала 5-й на 8-й,
    // потом вниз с 8-го на 3-й.
    let car = with_calls(idle(5, Up), &[8], &[]);
    assert_eq!(eta(&car, 3, Up), Some(drive(9000) + CYCLE + drive(15_000)));
}

#[test]
fn heading_down_mirrors_heading_up() {
    // Зеркало случая «подъём на 8-й, вызов вниз на 5-й»: 10-й этаж, спуск на 3-й, вызов
    // вверх на 6-й.
    let car = with_calls(idle(10, Down), &[], &[3]);
    assert_eq!(eta(&car, 6, Up), Some(drive(21_000) + CYCLE + drive(9000)));
}

#[test]
fn passenger_destination_is_an_extra_stop_on_the_way() {
    // Пассажир в кабине нажал 5-й этаж. Вызов вверх на 7-м ждёт остановки на 5-м.
    let car = with_car_calls(idle(1, Up), &[5]);
    assert_eq!(eta(&car, 7, Up), Some(drive(12_000) + CYCLE + drive(6000)));
}

#[test]
fn passenger_destination_below_does_not_delay_a_call_above() {
    let car = with_car_calls(idle(5, Up), &[2]);
    assert_eq!(eta(&car, 8, Up), Some(drive(9000)));
}

#[test]
fn passenger_destination_below_is_served_on_the_way_down() {
    // Кабина на 5-м, пассажир едет на 2-й, вызов вниз на 3-м по дороге: ждёт только проезда.
    let car = with_car_calls(idle(5, Up), &[2]);
    assert_eq!(eta(&car, 3, Down), Some(drive(6000)));
}

#[test]
fn hall_call_and_passenger_destination_on_one_floor_share_the_stop() {
    let car = with_car_calls(idle(1, Up), &[3]);
    assert_eq!(eta(&car, 3, Up), Some(drive(6000)));
}

#[test]
fn passenger_destination_above_stretches_the_sweep() {
    // Пассажир едет на 9-й, вызов вниз на 5-м: сначала до 9-го, потом вниз к 5-му.
    let car = with_car_calls(idle(1, Up), &[9]);
    assert_eq!(
        eta(&car, 5, Down),
        Some(drive(24_000) + CYCLE + drive(12_000))
    );
}

#[test]
fn matrix_shows_why_the_global_choice_beats_the_nearest_first_one() {
    // Кабина A стоит на 6-м этаже, B на 10-м, C перегружена на 1-м.
    // Вызовы по порядку регистрации: X это 3-й этаж вниз, Y это 8-й вверх.
    let cars = [
        idle(6, Up),
        idle(10, Down),
        CarState {
            mode: CarMode::Unavailable,
            ..idle(1, Up)
        },
    ];
    let calls = [queued(3, Down, 0), queued(8, Up, 0)];
    let matrix = build_matrix(&params(), &cars, &calls);
    assert_eq!(
        matrix,
        [
            [Cell::allowed(drive(9000)), Cell::allowed(drive(6000))],
            [Cell::allowed(drive(21_000)), Cell::allowed(drive(6000))],
            [Cell::forbidden(0), Cell::forbidden(0)],
        ]
    );
    // Жадный выбор: A берёт Y (кабины по id при равенстве), B остаётся с X, это дороже.
    // Глобально лучше A на X и B на Y.
    assert_eq!(assign(&matrix), [Some(0), Some(1), None]);
}

#[test]
fn long_waiting_call_wins_when_there_are_more_calls_than_cars() {
    // Одна кабина на 1-м этаже, два вызова: далёкий X давно ждёт, близкий Y свежий.
    let cars = [idle(1, Up)];
    let calls = [queued(8, Up, 60_000), queued(3, Up, 0)];

    let mut plain = params();
    plain.aging_cap_ms = 0;
    let plain_matrix = build_matrix(&plain, &cars, &calls);
    assert_eq!(
        plain_matrix,
        [[Cell::allowed(drive(21_000)), Cell::allowed(drive(6000))]]
    );
    assert_eq!(assign(&plain_matrix), [Some(1)]);

    let mut aging = params();
    aging.aging_cap_ms = 120_000;
    let aging_matrix = build_matrix(&aging, &cars, &calls);
    assert_eq!(
        aging_matrix,
        [[
            Cell::allowed(drive(21_000) + 60_000),
            Cell::allowed(drive(6000) + 120_000)
        ]]
    );
    assert_eq!(assign(&aging_matrix), [Some(0)]);
}

#[test]
fn waiting_longer_than_the_cap_counts_as_the_cap() {
    let cars = [idle(1, Up)];
    let mut aging = params();
    aging.aging_cap_ms = 10_000;
    let matrix = build_matrix(&aging, &cars, &[queued(3, Up, 999_999)]);
    assert_eq!(matrix, [[Cell::allowed(drive(6000))]]);
}

struct Rng(u64);

impl Rng {
    fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    // Число от нуля до `limit` не включая.
    fn below(&mut self, limit: u32) -> u32 {
        u32::try_from(self.step() % u64::from(limit)).expect("остаток меньше limit")
    }

    fn direction(&mut self) -> Direction {
        if self.below(2) == 0 { Up } else { Down }
    }

    // Редкая маска этажей: каждый бит с вероятностью около четверти.
    fn sparse_mask(&mut self) -> u32 {
        self.below(1 << FLOORS) & self.below(1 << FLOORS)
    }

    fn car(&mut self) -> CarState {
        let moving = self.below(3) == 0;
        let heading = self.direction();
        let (position_mm, speed_mm_s) = if moving {
            (self.below(pos(FLOORS) + 1), self.below(1600) + 1)
        } else {
            (pos(self.below(FLOORS) + 1), 0)
        };
        // Разгоняется примерно каждая вторая движущаяся кабина.
        let accel_mm_s2 = if moving && self.below(2) == 0 {
            self.below(1001)
        } else {
            0
        };
        CarState {
            position_mm,
            speed_mm_s,
            accel_mm_s2,
            heading,
            door_remaining_ms: self.below(10_000),
            up_calls: self.sparse_mask(),
            down_calls: self.sparse_mask(),
            car_calls: self.sparse_mask() & self.sparse_mask(),
            mode: CarMode::Normal,
        }
    }
}

// Зеркало по высоте здания: этажи переворачиваются, направления меняются.
fn mirror_mask(m: u32) -> u32 {
    m.reverse_bits() >> (32 - FLOORS)
}

const fn flip(direction: Direction) -> Direction {
    match direction {
        Up => Down,
        Down => Up,
    }
}

fn mirror(car: &CarState, target: Call) -> (CarState, Call) {
    (
        CarState {
            position_mm: pos(FLOORS) - car.position_mm,
            heading: flip(car.heading),
            up_calls: mirror_mask(car.down_calls),
            down_calls: mirror_mask(car.up_calls),
            car_calls: mirror_mask(car.car_calls),
            ..*car
        },
        Call {
            floor: u8::try_from(FLOORS + 1 - u32::from(target.floor)).expect("этаж"),
            direction: flip(target.direction),
        },
    )
}

// Вид вызова в эталоне: вызов этажа вверх или вниз, либо этаж, нажатый в кабине.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Up,
    Down,
    Car,
}

type Pending = Vec<(u32, Kind)>;

// Забирает из `remaining` вызовы на этаже `floor`, вид которых подходит под `keep`.
fn take(remaining: &mut Pending, floor: u32, keep: impl Fn(Kind) -> bool) -> Pending {
    let served: Pending = remaining
        .iter()
        .copied()
        .filter(|(f, k)| *f == floor && keep(*k))
        .collect();
    remaining.retain(|c| !served.contains(c));
    served
}

fn floors_of(calls: &Pending, keep: impl Fn(Kind) -> bool) -> Vec<u32> {
    calls
        .iter()
        .filter(|(_, k)| keep(*k))
        .map(|(floor, _)| *floor)
        .collect()
}

// Независимый эталон для кабины, едущей вверх: те же правила, но списками и сортировкой.
// Три прохода: вверх через попутные вызовы и вызовы из кабины до самой верхней точки,
// вниз через вызовы вниз и из кабины до самой нижней, снова вверх через оставшиеся вызовы вверх.
fn oracle_up(p: &EtaParams, car: &CarState, target: Call) -> Option<u32> {
    let target = (
        u32::from(target.floor),
        match target.direction {
            Up => Kind::Up,
            Down => Kind::Down,
        },
    );
    let mut pending: Pending = Vec::new();
    for floor in 1..=FLOORS {
        for (mask, kind) in [
            (car.up_calls, Kind::Up),
            (car.down_calls, Kind::Down),
            (car.car_calls, Kind::Car),
        ] {
            if (mask >> (floor - 1)) & 1 == 1 {
                pending.push((floor, kind));
            }
        }
    }
    if !pending.contains(&target) {
        pending.push(target);
    }

    let moving = car.speed_mm_s > 0 || car.accel_mm_s2 > 0;
    let stopping = p
        .motion
        .stopping_distance_mm(car.speed_mm_s, car.accel_mm_s2)?;
    let reach = car.position_mm + stopping;
    let ahead: Pending = pending
        .iter()
        .copied()
        .filter(|(floor, _)| pos(*floor) >= reach)
        .collect();

    // Остановка: этаж, пустая ли она (только торможение, двери не открываются), кого обслужила.
    let mut stops: Vec<(u32, bool, Pending)> = Vec::new();
    let mut remaining = pending.clone();

    if let Some(top) = ahead.iter().map(|(floor, _)| *floor).max() {
        let mut floors = floors_of(&ahead, |k| k != Kind::Down);
        floors.push(top);
        floors.sort_unstable();
        floors.dedup();
        for floor in floors {
            let served = take(&mut remaining, floor, |k| k != Kind::Down || floor == top);
            stops.push((floor, false, served));
        }
    } else if moving {
        let first = reach.div_ceil(HEIGHT) + 1;
        if first > FLOORS {
            return None;
        }
        stops.push((first, true, Vec::new()));
    }

    if let Some(lowest) = remaining.iter().map(|(floor, _)| *floor).min() {
        let mut floors = floors_of(&remaining, |k| k != Kind::Up);
        floors.push(lowest);
        floors.sort_unstable_by_key(|floor| std::cmp::Reverse(*floor));
        floors.dedup();
        for floor in floors {
            let served = take(&mut remaining, floor, |k| k != Kind::Up || floor == lowest);
            stops.push((floor, false, served));
        }
        let mut floors = floors_of(&remaining, |_| true);
        floors.sort_unstable();
        floors.dedup();
        for floor in floors {
            let served = take(&mut remaining, floor, |_| true);
            stops.push((floor, false, served));
        }
    }

    let mut clock = if moving {
        0
    } else {
        u64::from(car.door_remaining_ms)
    };
    let mut at = car.position_mm;
    let mut speed = car.speed_mm_s;
    let mut accel = car.accel_mm_s2;
    for (floor, is_virtual, served) in &stops {
        let travel = p
            .motion
            .time_to_stop_from_ms(pos(*floor).abs_diff(at), speed, accel)?;
        clock += u64::from(travel);
        at = pos(*floor);
        speed = 0;
        accel = 0;
        if served.contains(&target) {
            return Some(u32::try_from(clock).unwrap_or(u32::MAX));
        }
        if !*is_virtual {
            clock += p.doors.cycle_ms();
        }
    }
    None
}

#[test]
fn random_cars_match_the_oracle() {
    let p = params();
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for _ in 0..30_000 {
        let car = rng.car();
        let target = Call {
            floor: u8::try_from(rng.below(FLOORS) + 1).expect("этаж"),
            direction: rng.direction(),
        };
        let expected = match car.heading {
            Up => oracle_up(&p, &car, target),
            Down => {
                let (mirrored, mirrored_call) = mirror(&car, target);
                oracle_up(&p, &mirrored, mirrored_call)
            }
        };
        assert_eq!(
            eta_ms(&p, &car, target),
            expected,
            "кабина {car:?}, вызов {target:?}"
        );
    }
}

#[test]
fn a_standing_car_is_never_faster_than_driving_straight() {
    let p = params();
    let mut rng = Rng(7);
    for _ in 0..20_000 {
        let car = CarState {
            speed_mm_s: 0,
            accel_mm_s2: 0,
            position_mm: pos(rng.below(FLOORS) + 1),
            ..rng.car()
        };
        let target = Call {
            floor: u8::try_from(rng.below(FLOORS) + 1).expect("этаж"),
            direction: rng.direction(),
        };
        let straight =
            car.door_remaining_ms + drive(pos(u32::from(target.floor)).abs_diff(car.position_mm));
        let got = eta_ms(&p, &car, target).expect("стоящая кабина всегда оценивается");
        assert!(got >= straight, "кабина {car:?}, вызов {target:?}");
    }
}

#[test]
fn repeating_an_assigned_call_changes_nothing() {
    let car = with_calls(idle(1, Up), &[4], &[6]);
    let base = eta(&car, 6, Down);
    assert!(base.is_some());
    assert_eq!(eta(&with_calls(car, &[4], &[]), 6, Down), base);
}

#[test]
fn building_rejects_nonsense() {
    assert_eq!(Building::new(0, 3000), None);
    assert_eq!(Building::new(33, 3000), None);
    assert_eq!(Building::new(10, 0), None);
    assert_eq!(Building::new(32, u32::MAX), None);
    assert!(Building::new(32, 3000).is_some());
    assert!(Building::new(1, u32::MAX).is_some());
}

#[test]
fn thirty_two_floors_use_every_bit_of_the_masks() {
    let tall = EtaParams {
        building: Building::new(32, HEIGHT).expect("здание"),
        ..params()
    };
    let bottom = CarState {
        position_mm: 0,
        ..idle(1, Up)
    };
    let up = Call {
        floor: 32,
        direction: Up,
    };
    // 31 этаж по 3000 мм.
    assert_eq!(eta_ms(&tall, &bottom, up), Some(drive(93_000)));
    let top = CarState {
        position_mm: 31 * HEIGHT,
        heading: Down,
        ..bottom
    };
    let down = Call {
        floor: 1,
        direction: Down,
    };
    assert_eq!(eta_ms(&tall, &top, down), Some(drive(93_000)));
    // Назначенный вызов на 32-м этаже и тот же новый: маршрут не меняется.
    let busy = CarState {
        up_calls: 1 << 31,
        ..bottom
    };
    assert_eq!(eta_ms(&tall, &busy, up), Some(drive(93_000)));
}

#[test]
fn door_phases_only_matter_through_their_sum() {
    let split = EtaParams {
        doors: DoorTimes::new(1000, 2000, 8000).expect("двери"),
        ..params()
    };
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(
        eta_ms(&split, &car, call(7, Up)),
        eta_ms(&params(), &car, call(7, Up))
    );
}

#[test]
fn door_times_follow_the_en_81_70_dwell_range() {
    assert!(DoorTimes::new(2000, 6000, 3000).is_some());
    assert!(DoorTimes::new(2000, 2000, 3000).is_some());
    assert!(DoorTimes::new(2000, 20_000, 3000).is_some());
    assert_eq!(DoorTimes::new(2000, 1999, 3000), None);
    assert_eq!(DoorTimes::new(2000, 20_001, 3000), None);
    assert_eq!(DoorTimes::new(0, 6000, 3000), None);
    assert_eq!(DoorTimes::new(2000, 6000, 0), None);
    assert_eq!(
        DoorTimes::new(2000, 6000, 3000).map(DoorTimes::cycle_ms),
        Some(11_000)
    );
}

#[test]
fn only_a_car_in_normal_mode_gets_assignments() {
    for mode in [
        CarMode::FireRecall,
        CarMode::Firefighters,
        CarMode::Unavailable,
    ] {
        let car = CarState {
            mode,
            ..idle(1, Up)
        };
        assert_eq!(eta(&car, 3, Up), None, "{mode:?}");
        let matrix = build_matrix(&params(), &[car], &[queued(3, Up, 0)]);
        assert_eq!(matrix, [[Cell::forbidden(0)]], "{mode:?}");
    }
    assert!(CarMode::Normal.accepts_assignments());
}

#[test]
fn accelerating_car_needs_a_longer_road_to_stop() {
    // Кабина на уровне 2-го этажа идёт 1000 мм/с. Если ускорение нулевое, остановиться на
    // 3-м этаже (3000 мм) успевает: тормозной путь 1000 мм.
    let steady = CarState {
        position_mm: pos(2),
        speed_mm_s: 1000,
        ..idle(1, Up)
    };
    assert_eq!(eta(&steady, 3, Up), Some(drive_from(3000, 1000)));
    // Если она при этом разгоняется с 1000 мм/с², скорость ещё вырастет до 1500, и
    // остановка требует около 3208 мм: на 3-м этаже уже не встать. Ближайший этаж, где
    // успевает, 4-й, потом возврат на 3-й.
    let accelerating = CarState {
        accel_mm_s2: 1000,
        ..steady
    };
    let to_fourth = motion()
        .time_to_stop_from_ms(6000, 1000, 1000)
        .expect("успевает");
    assert_eq!(eta(&accelerating, 3, Up), Some(to_fourth + drive(3000)));
}

#[test]
fn impossible_acceleration_gets_no_estimate() {
    let too_hard = CarState {
        accel_mm_s2: 1001,
        ..flying()
    };
    assert_eq!(eta(&too_hard, 6, Up), None);
    // Скорость после гашения ускорения вышла бы за максимальную.
    let overshoot = CarState {
        accel_mm_s2: 1000,
        ..flying()
    };
    assert_eq!(eta(&overshoot, 6, Up), None);
}
