#![no_std]

pub mod calls;
pub mod dispatcher;
pub mod eta;
pub mod motion;

use core::ops::{Index, IndexMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub eta_ms: u64,
    pub allowed: bool,
}

impl Cell {
    #[must_use]
    pub const fn allowed(eta_ms: u32) -> Self {
        Self {
            eta_ms: eta_ms as u64,
            allowed: true,
        }
    }

    #[must_use]
    pub const fn forbidden(eta_ms: u32) -> Self {
        Self {
            eta_ms: eta_ms as u64,
            allowed: false,
        }
    }
}

#[must_use]
pub fn assign<const CARS: usize, const CALLS: usize>(
    matrix: &[[Cell; CALLS]; CARS],
) -> [Option<usize>; CARS] {
    let matched = solve(matrix);

    let mut result = [None; CARS];
    for column in 1..=CALLS {
        let row = matched[column];
        if row != 0 && matrix[row - 1][column - 1].allowed {
            result[row - 1] = Some(column - 1);
        }
    }
    result
}

#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
struct Cost {
    missed: i64,
    eta: i128,
    order: i128,
}

impl core::ops::Add for Cost {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            missed: self.missed + rhs.missed,
            eta: self.eta + rhs.eta,
            order: self.order + rhs.order,
        }
    }
}

impl core::ops::Sub for Cost {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            missed: self.missed - rhs.missed,
            eta: self.eta - rhs.eta,
            order: self.order - rhs.order,
        }
    }
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
) -> Slots<usize, CALLS, CARS> {
    let columns = CALLS + CARS;
    let zero = Cost::default();
    let infinity = Cost {
        missed: i64::MAX,
        ..zero
    };
    let mut digit_weights = [1_i128; CARS];
    for i in (1..CARS).rev() {
        digit_weights[i - 1] = digit_weights[i] * (CALLS as i128 + 1);
    }
    let mut row_potential = Slots::<Cost, CARS, 0>::filled(zero);
    let mut column_potential = Slots::<Cost, CALLS, CARS>::filled(zero);
    let mut matched = Slots::<usize, CALLS, CARS>::filled(0);
    let mut way = Slots::<usize, CALLS, CARS>::filled(0);

    for row in 1..=CARS {
        matched[0] = row;
        let mut current = 0;
        let mut slack = Slots::<Cost, CALLS, CARS>::filled(infinity);
        let mut visited = Slots::<bool, CALLS, CARS>::filled(false);

        loop {
            visited[current] = true;
            let owner = matched[current];
            let mut delta = infinity;
            let mut next = 0;
            for column in 1..=columns {
                if visited[column] {
                    continue;
                }
                let cell = (column <= CALLS).then(|| matrix[owner - 1][column - 1]);
                let weight = digit_weights[owner - 1];
                let edge = match cell {
                    Some(c) if c.allowed => Cost {
                        missed: 0,
                        eta: i128::from(c.eta_ms),
                        order: (column - 1) as i128 * weight,
                    },
                    _ => Cost {
                        missed: if cell.is_some() { 2 } else { 1 },
                        eta: 0,
                        order: CALLS as i128 * weight,
                    },
                };
                let cost = edge - row_potential[owner] - column_potential[column];
                if cost < slack[column] {
                    slack[column] = cost;
                    way[column] = current;
                }
                if slack[column] < delta {
                    delta = slack[column];
                    next = column;
                }
            }
            row_potential[row] = row_potential[row] + delta;
            for column in 1..=columns {
                if visited[column] {
                    row_potential[matched[column]] = row_potential[matched[column]] + delta;
                    column_potential[column] = column_potential[column] - delta;
                } else {
                    slack[column] = slack[column] - delta;
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
