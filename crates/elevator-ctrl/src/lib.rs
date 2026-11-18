#![no_std]
//! Диспетчер лифтов: назначение кабин на вызовы.
//!
//! Функция [`assign`] решает задачу назначения венгерским алгоритмом на целых числах,
//! без кучи и без `unsafe`. Размеры матрицы заданы типом, поэтому выделять память не нужно.

pub mod calls;
pub mod eta;
pub mod motion;

use core::ops::{Index, IndexMut};

/// Ячейка матрицы «кабина на вызов».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Расчётное время подачи в миллисекундах.
    pub eta_ms: u32,
    /// Можно ли назначить эту пару. Если `false`, число `eta_ms` не имеет значения:
    /// запрещённая пара не попадает в результат, какое бы число в ней ни стояло.
    pub allowed: bool,
}

impl Cell {
    /// Допустимая пара с заданным ETA.
    #[must_use]
    pub const fn allowed(eta_ms: u32) -> Self {
        Self {
            eta_ms,
            allowed: true,
        }
    }

    /// Запрещённая пара. Число нужно, чтобы проверять, что оно действительно игнорируется.
    #[must_use]
    pub const fn forbidden(eta_ms: u32) -> Self {
        Self {
            eta_ms,
            allowed: false,
        }
    }
}

/// Раздаёт вызовы кабинам за один проход.
///
/// Строки матрицы это кабины по возрастанию id. Столбцы это вызовы по времени
/// регистрации, при равном времени по id. Результат: для каждой кабины номер столбца
/// назначенного ей вызова или `None`. Кабина получает не больше одного вызова, вызов
/// достаётся не больше чем одной кабине.
///
/// Цель: сначала как можно больше допустимых пар, потом минимальная сумма ETA.
///
/// Среди равных по этим двум критериям берётся предпочтительное назначение. Сравнение
/// идёт по кабинам с первой: кабина с вызовом лучше кабины без вызова, при двух
/// вызовах лучше меньший номер столбца. Так результат на компьютере и на плате совпадает.
///
/// Суммы ETA могут не поместиться в `u32`, поэтому всё считается в `i128`. Ограничение:
/// `(CALLS + 1)^CARS * (CARS * 2^32 + 1)` должно помещаться в `i128`. Для трёх кабин
/// это выполняется при любом разумном числе вызовов (до миллионов).
#[must_use]
pub fn assign<const CARS: usize, const CALLS: usize>(
    matrix: &[[Cell; CALLS]; CARS],
) -> [Option<usize>; CARS] {
    let weights = Weights::new(CARS, CALLS);
    let matched = solve(matrix, &weights);

    // Строка matched[j] (с единицы) занимает столбец j (с единицы). Столбцы после
    // CALLS это заглушки «кабина без вызова».
    let mut result = [None; CARS];
    for column in 1..=CALLS {
        let row = matched[column];
        if row != 0 && matrix[row - 1][column - 1].allowed {
            result[row - 1] = Some(column - 1);
        }
    }
    result
}

// Веса, превращающие три критерия в одно целое число, которое достаточно минимизировать:
// 1) больше пар, 2) меньше сумма ETA, 3) предпочтение по порядку кабин и вызовов.
//
// Третий критерий это число в системе счисления с основанием `base`: цифра кабины i равна
// номеру выбранного вызова (или CALLS, если вызова нет), старшая цифра у первой кабины.
// Минимум такого числа и есть лексикографическое предпочтение из контракта.
struct Weights {
    base: i128,
    // Больше любой суммы цифр третьего критерия, поэтому ETA важнее порядка.
    scale: i128,
    // Больше любой разницы в сумме ETA и порядке, поэтому лишняя пара важнее всего.
    bonus: i128,
    cars: usize,
    calls: usize,
}

impl Weights {
    fn new(cars: usize, calls: usize) -> Self {
        let base = wide(calls) + 1;
        let scale = pow(base, cars);
        let bonus = (wide(cars) * i128::from(u32::MAX) + 1) * scale;
        Self {
            base,
            scale,
            bonus,
            cars,
            calls,
        }
    }

    // Стоимость того, что кабина `car` (с нуля) получает столбец `column` (с нуля).
    // Столбцы от `calls` и дальше это заглушки «без вызова».
    fn cost(&self, cell: Option<Cell>, car: usize, column: usize) -> i128 {
        let digit_weight = pow(self.base, self.cars - 1 - car);
        match cell {
            Some(cell) if cell.allowed => {
                i128::from(cell.eta_ms) * self.scale + wide(column) * digit_weight - self.bonus
            }
            // Запрещённая пара дороже заглушки, поэтому в оптимум не попадает: заглушек
            // не меньше, чем кабин, и свободная всегда найдётся.
            Some(_) => self.bonus,
            None => wide(self.calls) * digit_weight,
        }
    }
}

const fn wide(n: usize) -> i128 {
    n as i128
}

const fn pow(base: i128, exp: usize) -> i128 {
    let mut result = 1;
    let mut left = exp;
    while left > 0 {
        result *= base;
        left -= 1;
    }
    result
}

// Массив с «нулевым» служебным элементом спереди и двумя частями (столбцы-вызовы и
// столбцы-заглушки). Нужен, потому что на стабильном Rust нельзя написать
// `[T; CALLS + CARS + 1]`. Индекс с нуля это служебный элемент, дальше сначала `first`,
// потом `second`.
struct Slots<T, const A: usize, const B: usize> {
    head: T,
    first: [T; A],
    second: [T; B],
}

impl<T: Copy, const A: usize, const B: usize> Slots<T, A, B> {
    const fn filled(value: T) -> Self {
        Self {
            head: value,
            first: [value; A],
            second: [value; B],
        }
    }
}

impl<T, const A: usize, const B: usize> Index<usize> for Slots<T, A, B> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        if index == 0 {
            &self.head
        } else if index <= A {
            &self.first[index - 1]
        } else {
            &self.second[index - A - 1]
        }
    }
}

impl<T, const A: usize, const B: usize> IndexMut<usize> for Slots<T, A, B> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        if index == 0 {
            &mut self.head
        } else if index <= A {
            &mut self.first[index - 1]
        } else {
            &mut self.second[index - A - 1]
        }
    }
}

// Венгерский алгоритм (потенциалы и дополняющие цепочки), строки и столбцы с единицы.
// Строк CARS, столбцов CALLS + CARS (после реальных вызовов идут заглушки по числу кабин).
// Возвращает для каждого столбца номер занявшей его строки, ноль если столбец свободен.
fn solve<const CARS: usize, const CALLS: usize>(
    matrix: &[[Cell; CALLS]; CARS],
    weights: &Weights,
) -> Slots<usize, CALLS, CARS> {
    let columns = CALLS + CARS;
    let mut row_potential = Slots::<i128, CARS, 0>::filled(0);
    let mut column_potential = Slots::<i128, CALLS, CARS>::filled(0);
    let mut matched = Slots::<usize, CALLS, CARS>::filled(0);
    let mut way = Slots::<usize, CALLS, CARS>::filled(0);

    for row in 1..=CARS {
        matched[0] = row;
        let mut current = 0;
        let mut slack = Slots::<i128, CALLS, CARS>::filled(i128::MAX);
        let mut visited = Slots::<bool, CALLS, CARS>::filled(false);

        loop {
            visited[current] = true;
            let owner = matched[current];
            let mut delta = i128::MAX;
            let mut next = 0;
            for column in 1..=columns {
                if visited[column] {
                    continue;
                }
                let cell = (column <= CALLS).then(|| matrix[owner - 1][column - 1]);
                let cost = weights.cost(cell, owner - 1, column - 1)
                    - row_potential[owner]
                    - column_potential[column];
                if cost < slack[column] {
                    slack[column] = cost;
                    way[column] = current;
                }
                if slack[column] < delta {
                    delta = slack[column];
                    next = column;
                }
            }
            for column in 0..=columns {
                if visited[column] {
                    row_potential[matched[column]] += delta;
                    column_potential[column] -= delta;
                } else {
                    slack[column] -= delta;
                }
            }
            current = next;
            if matched[current] == 0 {
                break;
            }
        }

        loop {
            let previous = way[current];
            matched[current] = matched[previous];
            current = previous;
            if current == 0 {
                break;
            }
        }
    }
    matched
}
