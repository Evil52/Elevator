//! Расчётное время подачи (ETA): за сколько миллисекунд кабина доедет до этажа вызова
//! и точно остановится там, до начала открытия дверей.
//!
//! Кабины работают по собирательному принципу: по пути берут попутные вызовы, а встречные
//! только в точке разворота. Оценщик строит маршрут кабины (подъём, спуск, снова подъём)
//! и считает время до остановки, на которой обслуживается нужный вызов.

use crate::Cell;
use crate::motion::MotionParams;

/// Направление вызова или движения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

/// Вызов этажа: этаж с единицы и направление кнопки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub floor: u8,
    pub direction: Direction,
}

/// Здание: число этажей (от 1 до 32) и высота этажа.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    floors: u8,
    floor_height_mm: u32,
}

impl Building {
    /// `None`, если этажей 0 или больше 32, высота этажа нулевая или шахта длиннее `u32`.
    #[must_use]
    pub const fn new(floors: u8, floor_height_mm: u32) -> Option<Self> {
        if floors == 0 || floors > 32 || floor_height_mm == 0 {
            return None;
        }
        if (floors as u32 - 1).checked_mul(floor_height_mm).is_none() {
            return None;
        }
        Some(Self {
            floors,
            floor_height_mm,
        })
    }

    // Положение этажа (с единицы) в мм.
    const fn position_mm(self, floor: u32) -> u32 {
        (floor - 1) * self.floor_height_mm
    }

    const fn top_position_mm(self) -> u32 {
        self.position_mm(self.floors as u32)
    }

    // Маска всех этажей здания.
    const fn all_floors(self) -> u32 {
        if self.floors == 32 {
            u32::MAX
        } else {
            (1 << self.floors) - 1
        }
    }

    // Переворачивает маску этажей: первый этаж становится последним.
    const fn mirror_mask(self, mask: u32) -> u32 {
        mask.reverse_bits() >> (32 - self.floors as u32)
    }
}

/// Всё, что нужно оценщику, кроме состояния кабин.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EtaParams {
    pub building: Building,
    pub motion: MotionParams,
    /// Полный цикл дверей на остановке: открытие, выдержка, закрытие.
    pub door_cycle_ms: u32,
}

/// Состояние кабины для оценки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarState {
    /// Положение в мм от нижнего этажа (1-й этаж это 0).
    pub position_mm: u32,
    /// Модуль скорости, мм/с. Ноль, если кабина стоит.
    pub speed_mm_s: u32,
    /// Куда едет кабина, а если стоит, то куда пойдёт.
    pub heading: Direction,
    /// Сколько ещё нельзя трогаться из-за дверей. Для движущейся кабины не учитывается.
    pub door_remaining_ms: u32,
    /// Уже назначенные вызовы «вверх»: бит `этаж - 1`.
    pub up_calls: u32,
    /// Уже назначенные вызовы «вниз»: бит `этаж - 1`.
    pub down_calls: u32,
    /// Может ли кабина получать назначения (не перегружена, замки подтверждены и так далее).
    pub available: bool,
}

/// ETA кабины до вызова в миллисекундах. `None`, если кабина недоступна или состояние
/// невозможно: этаж вне здания, положение за шахтой, скорость выше максимальной,
/// назначения на несуществующие этажи, остановиться в шахте нельзя.
#[must_use]
pub fn eta_ms(params: &EtaParams, car: &CarState, call: Call) -> Option<u32> {
    let building = &params.building;
    let floor = u32::from(call.floor);
    let valid = car.available
        && (1..=u32::from(building.floors)).contains(&floor)
        && car.position_mm <= building.top_position_mm()
        && car.speed_mm_s <= params.motion.max_speed_mm_s()
        && (car.up_calls | car.down_calls) & !building.all_floors() == 0;
    if !valid {
        return None;
    }
    match car.heading {
        Direction::Up => Route::new(params, car, call).run(),
        Direction::Down => {
            // Спуск считается как подъём в зеркальном здании: этажи и направления перевёрнуты.
            let mirrored = CarState {
                position_mm: building.top_position_mm() - car.position_mm,
                heading: Direction::Up,
                up_calls: building.mirror_mask(car.down_calls),
                down_calls: building.mirror_mask(car.up_calls),
                ..*car
            };
            let mirrored_call = Call {
                floor: building.floors + 1 - call.floor,
                direction: call.direction.opposite(),
            };
            Route::new(params, &mirrored, mirrored_call).run()
        }
    }
}

fn saturate(millis: u64) -> u32 {
    u32::try_from(millis).unwrap_or(u32::MAX)
}

// Время и положение кабины по ходу маршрута.
struct Clock<'a> {
    params: &'a EtaParams,
    millis: u64,
    position_mm: u32,
    speed_mm_s: u32,
}

impl Clock<'_> {
    // Едет к этажу и останавливается там. Возвращает момент остановки. Скорость после
    // остановки нулевая, поэтому начальная скорость влияет только на первый перегон.
    fn drive_to(&mut self, floor: u32) -> Option<u64> {
        let target = self.params.building.position_mm(floor);
        let travel = self
            .params
            .motion
            .time_to_stop_ms(target.abs_diff(self.position_mm), self.speed_mm_s)?;
        self.millis += u64::from(travel);
        self.position_mm = target;
        self.speed_mm_s = 0;
        Some(self.millis)
    }

    fn open_doors(&mut self) {
        self.millis += u64::from(self.params.door_cycle_ms);
    }
}

const fn floor_bit(floor: u32) -> u32 {
    1 << (floor - 1)
}

// Маршрут кабины, которая едет вверх (спуск сводится к нему зеркалом). Три прохода:
// вверх через попутные вызовы до самого верхнего, вниз через вызовы вниз до самого нижнего,
// снова вверх через оставшиеся вызовы вверх. Встречный вызов берётся только в точке разворота.
struct Route<'a> {
    clock: Clock<'a>,
    call: Call,
    floors: u32,
    // Первый этаж, на котором кабина ещё успевает остановиться.
    first_ahead: u32,
    start_position_mm: u32,
    up: u32,
    down: u32,
    // Этаж, где кабина развернулась после прохода вверх, если разворот был.
    turn_floor: Option<u32>,
    // Самый нижний этаж прохода вниз.
    lowest: u32,
    // ETA обслуживания нужного вызова, когда маршрут до него дошёл.
    found: Option<u32>,
}

impl<'a> Route<'a> {
    fn new(params: &'a EtaParams, car: &CarState, call: Call) -> Self {
        let building = &params.building;
        let target = floor_bit(u32::from(call.floor));
        let (mut up, mut down) = (car.up_calls, car.down_calls);
        match call.direction {
            Direction::Up => up |= target,
            Direction::Down => down |= target,
        }
        let moving = car.speed_mm_s > 0;
        let reach = u64::from(car.position_mm)
            + u64::from(params.motion.braking_distance_mm(car.speed_mm_s));
        let first_ahead = reach.div_ceil(u64::from(building.floor_height_mm)) + 1;
        Self {
            clock: Clock {
                params,
                // Двери движущейся кабины закрыты, остаток цикла нужен только стоящей.
                millis: if moving {
                    0
                } else {
                    u64::from(car.door_remaining_ms)
                },
                position_mm: car.position_mm,
                speed_mm_s: car.speed_mm_s,
            },
            call,
            floors: u32::from(building.floors),
            first_ahead: saturate(first_ahead),
            start_position_mm: car.position_mm,
            up,
            down,
            turn_floor: None,
            lowest: 0,
            found: None,
        }
    }

    fn run(mut self) -> Option<u32> {
        self.sweep_up()?;
        if self.found.is_none() {
            self.sweep_down()?;
        }
        if self.found.is_none() {
            self.sweep_up_again()?;
        }
        self.found
    }

    // Остановка на этаже: едем, и если здесь обслуживается нужный вызов, маршрут найден,
    // иначе проходит цикл дверей.
    fn visit(&mut self, floor: u32, served_up: bool, served_down: bool) -> Option<()> {
        let arrival = self.clock.drive_to(floor)?;
        let served = match self.call.direction {
            Direction::Up => served_up,
            Direction::Down => served_down,
        };
        if floor == u32::from(self.call.floor) && served {
            self.found = Some(saturate(arrival));
        } else {
            self.clock.open_doors();
        }
        Some(())
    }

    fn sweep_up(&mut self) -> Option<()> {
        let ahead = self.up_ahead_mask();
        let pending = (self.up | self.down) & ahead;
        if pending == 0 {
            // Впереди дел нет. Движущаяся кабина всё равно должна остановиться: тормозит
            // на ближайшем этаже, куда успевает, без открытия дверей.
            if self.clock.speed_mm_s > 0 {
                if self.first_ahead > self.floors {
                    return None;
                }
                self.clock.drive_to(self.first_ahead)?;
                self.turn_floor = Some(self.first_ahead);
            }
            return Some(());
        }
        let top = 32 - pending.leading_zeros();
        self.turn_floor = Some(top);
        for floor in self.first_ahead..=top {
            let bit = floor_bit(floor);
            let at_top = floor == top;
            if self.up & bit != 0 || at_top {
                self.visit(floor, self.up & bit != 0, at_top && self.down & bit != 0)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.up &= !bit;
                if at_top {
                    self.down &= !bit;
                }
            }
        }
        Some(())
    }

    // Этажи, на которых кабина ещё успевает остановиться при движении вверх.
    fn up_ahead_mask(&self) -> u32 {
        let all = self.clock.params.building.all_floors();
        all & u32::MAX.checked_shl(self.first_ahead - 1).unwrap_or(0)
    }

    fn sweep_down(&mut self) -> Option<()> {
        let building = &self.clock.params.building;
        let turn_position = self
            .turn_floor
            .map_or(self.start_position_mm, |floor| building.position_mm(floor));
        // Сколько этажей лежит строго ниже точки разворота.
        let below = turn_position.div_ceil(building.floor_height_mm);
        let below_mask = u32::MAX.checked_shr(32 - below.min(32)).unwrap_or(0);
        let rest_down = self.down & below_mask;
        let rest = rest_down | self.up;
        if rest == 0 {
            return None;
        }
        self.lowest = rest.trailing_zeros() + 1;
        for floor in (self.lowest..=below).rev() {
            let bit = floor_bit(floor);
            let at_lowest = floor == self.lowest;
            if rest_down & bit != 0 || at_lowest {
                self.visit(floor, at_lowest && self.up & bit != 0, rest_down & bit != 0)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.down &= !bit;
                if at_lowest {
                    self.up &= !bit;
                }
            }
        }
        Some(())
    }

    fn sweep_up_again(&mut self) -> Option<()> {
        for floor in (self.lowest + 1)..=self.floors {
            let bit = floor_bit(floor);
            if self.up & bit != 0 {
                self.visit(floor, true, false)?;
                if self.found.is_some() {
                    return Some(());
                }
                self.up &= !bit;
            }
        }
        None
    }
}

/// Матрица «кабина на вызов» для `assign`. Недоступная кабина или невозможная оценка
/// дают запрещённую ячейку.
#[must_use]
pub fn build_matrix<const CARS: usize, const CALLS: usize>(
    params: &EtaParams,
    cars: &[CarState; CARS],
    calls: &[Call; CALLS],
) -> [[Cell; CALLS]; CARS] {
    core::array::from_fn(|car| {
        core::array::from_fn(|call| match eta_ms(params, &cars[car], calls[call]) {
            Some(eta) => Cell::allowed(eta),
            None => Cell::forbidden(0),
        })
    })
}
