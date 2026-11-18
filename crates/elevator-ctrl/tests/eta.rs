//! Тесты расчёта ETA.
//!
//! Здание: 10 этажей по 3000 мм, этаж `n` лежит на `(n - 1) * 3000` мм. Движение: 3000 мм/с
//! и 1500 мм/с². Цикл дверей на остановке 6000 мс (в тестах число условное, не из ТЗ).
//! Время проезда между остановками считаем по модулю `motion`: 6000 мм это 4000 мс,
//! 9000 мм это 5000 мс, 3000 мм это 2829 мс, 15 000 мм это 7000 мс, 21 000 мм это 9000 мс.

use elevator_ctrl::eta::{Building, Call, CarState, Direction, EtaParams, build_matrix, eta_ms};
use elevator_ctrl::motion::MotionParams;
use elevator_ctrl::{Cell, assign};

use Direction::{Down, Up};

const FLOORS: u32 = 10;
const HEIGHT: u32 = 3000;
const DOOR_CYCLE: u32 = 6000;

fn params() -> EtaParams {
    EtaParams {
        building: Building::new(10, HEIGHT).expect("здание"),
        motion: MotionParams::new(3000, 1500).expect("движение"),
        door_cycle_ms: DOOR_CYCLE,
    }
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
        heading,
        door_remaining_ms: 0,
        up_calls: 0,
        down_calls: 0,
        available: true,
    }
}

fn with_calls(car: CarState, ups: &[u32], downs: &[u32]) -> CarState {
    CarState {
        up_calls: mask(ups),
        down_calls: mask(downs),
        ..car
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

#[test]
fn idle_car_drives_straight_to_the_call() {
    // 1-й этаж на 3-й: 6000 мм.
    assert_eq!(eta(&idle(1, Up), 3, Up), Some(4000));
    // Направление кнопки не важно, если у кабины нет других дел.
    assert_eq!(eta(&idle(1, Up), 3, Down), Some(4000));
}

#[test]
fn doors_still_closing_delay_the_start() {
    let car = CarState {
        door_remaining_ms: 1500,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 3, Up), Some(5500));
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
    // 5-й этаж на 1-й: 12 000 мм, 2 с разгона, 4 с хода, но тормоз и разгон внутри: 6000 мс.
    assert_eq!(eta(&idle(5, Up), 1, Down), Some(6000));
    assert_eq!(eta(&idle(10, Down), 7, Down), Some(5000));
}

#[test]
fn unavailable_car_gets_no_estimate() {
    let car = CarState {
        available: false,
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
        speed_mm_s: 3001,
        ..idle(1, Up)
    };
    assert_eq!(eta(&too_fast, 5, Up), None);
    let ghost_floor = CarState {
        up_calls: 1 << 10,
        ..car
    };
    assert_eq!(eta(&ghost_floor, 5, Up), None);
}

#[test]
fn moving_car_reaches_a_call_ahead() {
    // Едет вверх на полной скорости с 2-го этажа (3000 мм), цель 6-й (15 000 мм): ход 9000 мм
    // за 3 с и торможение 2 с.
    let car = CarState {
        position_mm: 3000,
        speed_mm_s: 3000,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 6, Up), Some(5000));
    // Ровно тормозной путь до 3-го этажа: 2 с торможения.
    assert_eq!(eta(&car, 3, Up), Some(2000));
}

#[test]
fn moving_car_ignores_the_door_timer() {
    let car = CarState {
        position_mm: 3000,
        speed_mm_s: 3000,
        door_remaining_ms: 9999,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 6, Up), Some(5000));
}

#[test]
fn moving_car_cannot_stop_at_a_floor_it_is_already_passing() {
    // Кабина на уровне 2-го этажа на полной скорости, вызов вверх на 2-м этаже. Ближайший
    // этаж, где можно встать, 3-й: 2000 мс. Там разворот и возврат вниз: 3000 мм, 2829 мс.
    let car = CarState {
        position_mm: 3000,
        speed_mm_s: 3000,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 2, Up), Some(4829));
}

#[test]
fn moving_car_past_the_last_reachable_floor_gets_no_estimate() {
    let car = CarState {
        position_mm: pos(9) + 2000,
        speed_mm_s: 3000,
        ..idle(1, Up)
    };
    assert_eq!(eta(&car, 5, Down), None);
}

#[test]
fn on_the_way_call_is_not_delayed_by_a_later_stop() {
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 3, Up), Some(4000));
}

#[test]
fn on_the_way_call_beyond_a_planned_stop_waits_for_its_doors() {
    // 1-й на 4-й (9000 мм) 5000 мс, двери 6000, 4-й на 7-й (9000 мм) 5000 мс.
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 7, Up), Some(16_000));
}

#[test]
fn opposite_call_is_taken_at_the_turnaround_point() {
    // Есть вызов вверх на 4-й, новый вызов вниз на 6-й: кабина поднимается до 6-го,
    // это точка разворота. 5000 + 6000 + 4000 (6000 мм).
    let car = with_calls(idle(1, Up), &[4], &[]);
    assert_eq!(eta(&car, 6, Down), Some(15_000));
}

#[test]
fn opposite_call_below_the_turnaround_is_taken_on_the_way_back() {
    // Подъём на 8-й (21 000 мм) 9000 мс, двери 6000, спуск на 5-й (9000 мм) 5000 мс.
    let car = with_calls(idle(1, Up), &[8], &[]);
    assert_eq!(eta(&car, 5, Down), Some(20_000));
}

#[test]
fn call_in_the_same_direction_behind_waits_for_the_next_sweep() {
    // Едет вверх с 5-го этажа к 8-му, вызов вверх на 3-м остался позади: сначала 5-й на 8-й
    // (9000 мм) 5000 мс, двери 6000, потом вниз с 8-го на 3-й (15 000 мм) 7000 мс.
    let car = with_calls(idle(5, Up), &[8], &[]);
    assert_eq!(eta(&car, 3, Up), Some(18_000));
}

#[test]
fn heading_down_mirrors_heading_up() {
    // Зеркало случая «подъём на 8-й, вызов вниз на 5-й»: 10-й этаж, спуск на 3-й, вызов вверх на 6-й.
    let car = with_calls(idle(10, Down), &[], &[3]);
    assert_eq!(eta(&car, 6, Up), Some(20_000));
}

#[test]
fn matrix_shows_why_the_global_choice_beats_the_nearest_first_one() {
    // Кабина A стоит на 6-м этаже, B на 10-м, C перегружена на 1-м.
    // Вызовы по порядку регистрации: X это 3-й этаж вниз, Y это 8-й вверх.
    let cars = [
        idle(6, Up),
        idle(10, Down),
        CarState {
            available: false,
            ..idle(1, Up)
        },
    ];
    let calls = [call(3, Down), call(8, Up)];
    let matrix = build_matrix(&params(), &cars, &calls);
    assert_eq!(
        matrix,
        [
            [Cell::allowed(5000), Cell::allowed(4000)],
            [Cell::allowed(9000), Cell::allowed(4000)],
            [Cell::forbidden(0), Cell::forbidden(0)],
        ]
    );
    // Жадный выбор: A берёт Y (4000), B остаётся с X (9000), всего 13 000.
    // Глобально лучше A на X и B на Y: 5000 + 4000 = 9000.
    assert_eq!(assign(&matrix), [Some(0), Some(1), None]);
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
            (self.below(pos(FLOORS) + 1), self.below(3000) + 1)
        } else {
            (pos(self.below(FLOORS) + 1), 0)
        };
        CarState {
            position_mm,
            speed_mm_s,
            heading,
            door_remaining_ms: self.below(7000),
            up_calls: self.sparse_mask(),
            down_calls: self.sparse_mask(),
            available: true,
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
            ..*car
        },
        Call {
            floor: u8::try_from(FLOORS + 1 - u32::from(target.floor)).expect("этаж"),
            direction: flip(target.direction),
        },
    )
}

type Pending = Vec<(u32, Direction)>;

// Забирает из `remaining` вызовы на этаже `floor`, у которых направление подходит под `keep`.
fn take(remaining: &mut Pending, floor: u32, keep: impl Fn(Direction) -> bool) -> Pending {
    let served: Pending = remaining
        .iter()
        .copied()
        .filter(|(f, d)| *f == floor && keep(*d))
        .collect();
    remaining.retain(|c| !served.contains(c));
    served
}

fn floors_of(calls: &Pending, keep: impl Fn(Direction) -> bool) -> Vec<u32> {
    calls
        .iter()
        .filter(|(_, d)| keep(*d))
        .map(|(floor, _)| *floor)
        .collect()
}

// Независимый эталон для кабины, едущей вверх: те же правила, но списками и сортировкой.
// Три прохода: вверх через попутные вызовы до самой верхней точки, вниз через вызовы вниз
// до самой нижней, снова вверх через оставшиеся вызовы вверх.
fn oracle_up(p: &EtaParams, car: &CarState, target: Call) -> Option<u32> {
    let target = (u32::from(target.floor), target.direction);
    let mut pending: Pending = Vec::new();
    for floor in 1..=FLOORS {
        if (car.up_calls >> (floor - 1)) & 1 == 1 {
            pending.push((floor, Up));
        }
        if (car.down_calls >> (floor - 1)) & 1 == 1 {
            pending.push((floor, Down));
        }
    }
    if !pending.contains(&target) {
        pending.push(target);
    }

    let moving = car.speed_mm_s > 0;
    let reach = car.position_mm + p.motion.braking_distance_mm(car.speed_mm_s);
    let ahead: Pending = pending
        .iter()
        .copied()
        .filter(|(floor, _)| pos(*floor) >= reach)
        .collect();

    // Остановка: этаж, пустая ли она (только торможение, двери не открываются), кого обслужила.
    let mut stops: Vec<(u32, bool, Pending)> = Vec::new();
    let mut remaining = pending.clone();

    if let Some(top) = ahead.iter().map(|(floor, _)| *floor).max() {
        let mut floors = floors_of(&ahead, |d| d == Up);
        floors.push(top);
        floors.sort_unstable();
        floors.dedup();
        for floor in floors {
            let served = take(&mut remaining, floor, |d| d == Up || floor == top);
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
        let mut floors = floors_of(&remaining, |d| d == Down);
        floors.push(lowest);
        floors.sort_unstable_by_key(|floor| std::cmp::Reverse(*floor));
        floors.dedup();
        for floor in floors {
            let served = take(&mut remaining, floor, |d| d == Down || floor == lowest);
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
    for (floor, is_virtual, served) in &stops {
        let travel = p.motion.time_to_stop_ms(pos(*floor).abs_diff(at), speed)?;
        clock += u64::from(travel);
        at = pos(*floor);
        speed = 0;
        if served.contains(&target) {
            return Some(u32::try_from(clock).unwrap_or(u32::MAX));
        }
        if !*is_virtual {
            clock += u64::from(DOOR_CYCLE);
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
            position_mm: pos(rng.below(FLOORS) + 1),
            ..rng.car()
        };
        let target = Call {
            floor: u8::try_from(rng.below(FLOORS) + 1).expect("этаж"),
            direction: rng.direction(),
        };
        let straight = car.door_remaining_ms
            + p.motion
                .time_to_stop_ms(pos(u32::from(target.floor)).abs_diff(car.position_mm), 0)
                .expect("с места доехать можно");
        let got = eta_ms(&p, &car, target).expect("стоящая кабина всегда оценивается");
        assert!(got >= straight, "кабина {car:?}, вызов {target:?}");
    }
}

#[test]
fn estimate_does_not_depend_on_calls_already_assigned_being_repeated() {
    // Повторное назначение того же вызова ничего не меняет.
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
    // 31 этаж по 3000 мм = 93 000 мм: 2 с разгона, 31 с хода, торможение внутри: 33 000 мс.
    let up = Call {
        floor: 32,
        direction: Up,
    };
    assert_eq!(eta_ms(&tall, &bottom, up), Some(33_000));
    let top = CarState {
        position_mm: 31 * HEIGHT,
        heading: Down,
        ..bottom
    };
    let down = Call {
        floor: 1,
        direction: Down,
    };
    assert_eq!(eta_ms(&tall, &top, down), Some(33_000));
    // Назначенный вызов на 32-м этаже и новый внизу: тот же маршрут, что и с зеркалом.
    let busy = CarState {
        up_calls: 1 << 31,
        ..bottom
    };
    assert_eq!(eta_ms(&tall, &busy, up), Some(33_000));
}
