#![no_std]

pub mod calls;
pub mod dispatcher;
pub mod eta;
pub mod motion;

use core::ops::{Index, IndexMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub eta_ms: u32,
    pub allowed: bool,
}

impl Cell {
    #[must_use]
    pub const fn allowed(eta_ms: u32) -> Self {
        Self {
            eta_ms,
            allowed: true,
        }
    }

    #[must_use]
    pub const fn forbidden(eta_ms: u32) -> Self {
        Self {
            eta_ms,
            allowed: false,
        }
    }
}

#[must_use]
pub fn assign<const CARS: usize, const CALLS: usize>(
    matrix: &[[Cell; CALLS]; CARS],
) -> [Option<usize>; CARS] {
    let weights = Weights::new(CARS, CALLS);
    let matched = solve(matrix, &weights);

    let mut result = [None; CARS];
    for column in 1..=CALLS {
        let row = matched[column];
        if row != 0 && matrix[row - 1][column - 1].allowed {
            result[row - 1] = Some(column - 1);
        }
    }
    result
}

struct Weights {
    base: i128,
    scale: i128,
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

    fn cost(&self, cell: Option<Cell>, car: usize, column: usize) -> i128 {
        let digit_weight = pow(self.base, self.cars - 1 - car);
        match cell {
            Some(cell) if cell.allowed => {
                i128::from(cell.eta_ms) * self.scale + wide(column) * digit_weight - self.bonus
            }
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
