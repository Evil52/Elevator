//! Тесты назначения кабин на вызовы.
//!
//! Эталон `oracle` перебирает все допустимые назначения и выбирает лучшее по правилам
//! из README: сначала больше пар, потом меньше сумма ETA, потом предпочтение по порядку.
//! Реализация `assign` должна давать тот же ответ.

use elevator_ctrl::{Cell, assign};

type Matrix<const C: usize, const K: usize> = [[Cell; K]; C];
type Picks<const C: usize> = [Option<usize>; C];

const fn ok(eta_ms: u32) -> Cell {
    Cell::allowed(eta_ms)
}

const fn no(eta_ms: u32) -> Cell {
    Cell::forbidden(eta_ms)
}

// Сколько пар назначено и чему равна сумма ETA (в u64, чтобы не переполниться).
fn totals<const C: usize, const K: usize>(m: &Matrix<C, K>, picks: &Picks<C>) -> (usize, u64) {
    let mut count = 0;
    let mut sum = 0_u64;
    for (row, pick) in m.iter().zip(picks) {
        if let Some(call) = pick {
            count += 1;
            sum += u64::from(row[*call].eta_ms);
        }
    }
    (count, sum)
}

// Предпочтение при равенстве числа пар и суммы: идём по кабинам с первой,
// кабина с вызовом лучше кабины без вызова, меньший номер вызова лучше большего.
fn prefers<const C: usize>(a: &Picks<C>, b: &Picks<C>) -> bool {
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (Some(p), Some(q)) if p != q => return p < q,
            (Some(_), None) => return true,
            (None, Some(_)) => return false,
            _ => {}
        }
    }
    false
}

fn better<const C: usize, const K: usize>(m: &Matrix<C, K>, a: &Picks<C>, b: &Picks<C>) -> bool {
    let (count_a, sum_a) = totals(m, a);
    let (count_b, sum_b) = totals(m, b);
    if count_a != count_b {
        return count_a > count_b;
    }
    if sum_a != sum_b {
        return sum_a < sum_b;
    }
    prefers(a, b)
}

fn search<const C: usize, const K: usize>(
    m: &Matrix<C, K>,
    car: usize,
    current: &mut Picks<C>,
    best: &mut Option<Picks<C>>,
) {
    if car == C {
        if best.as_ref().is_none_or(|b| better(m, current, b)) {
            *best = Some(*current);
        }
        return;
    }
    for call in 0..K {
        if m[car][call].allowed && !current[..car].contains(&Some(call)) {
            current[car] = Some(call);
            search(m, car + 1, current, best);
        }
    }
    current[car] = None;
    search(m, car + 1, current, best);
}

fn oracle<const C: usize, const K: usize>(m: &Matrix<C, K>) -> Picks<C> {
    let mut best = None;
    search(m, 0, &mut [None; C], &mut best);
    best.expect("перебор всегда находит хотя бы пустое назначение")
}

// Назначение корректно: вызов один раз, только по допустимым парам.
fn assert_valid<const C: usize, const K: usize>(m: &Matrix<C, K>, picks: &Picks<C>) {
    for (car, (row, pick)) in m.iter().zip(picks).enumerate() {
        if let Some(call) = pick {
            assert!(*call < K, "кабина {car}: вызов {call} вне матрицы");
            assert!(
                row[*call].allowed,
                "кабина {car}: назначена запрещённая пара с вызовом {call}"
            );
            let times = picks.iter().filter(|p| **p == Some(*call)).count();
            assert_eq!(times, 1, "вызов {call} назначен {times} раз");
        }
    }
}

// Ожидание, посчитанное руками, должно совпасть с эталоном: иначе ошибка в самом тесте.
fn check<const C: usize, const K: usize>(m: &Matrix<C, K>, expected: Picks<C>) {
    assert_eq!(
        oracle(m),
        expected,
        "ошибка в тесте: эталон не совпал с ожиданием"
    );
    let got = assign(m);
    assert_valid(m, &got);
    assert_eq!(got, expected, "матрица: {m:?}");
}

struct Rng(u64);

impl Rng {
    fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u32) -> u32 {
        u32::try_from(self.step() % u64::from(limit)).expect("остаток меньше limit")
    }
}

// Случайные матрицы с малым разбросом ETA (много ничьих) и четвертью запрещённых пар.
// У запрещённых ETA тоже случайный, часто нулевой: он не должен влиять на ответ.
fn fuzz<const C: usize, const K: usize>(seed: u64, cases: u32) {
    let mut rng = Rng(seed);
    for _ in 0..cases {
        let m: Matrix<C, K> = std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                let eta_ms = rng.below(7);
                if rng.below(4) == 0 {
                    no(eta_ms)
                } else {
                    ok(eta_ms)
                }
            })
        });
        let got = assign(&m);
        assert_valid(&m, &got);
        assert_eq!(got, oracle(&m), "матрица: {m:?}");
    }
}

#[test]
fn no_calls_gives_no_assignments() {
    let m: Matrix<3, 0> = [[], [], []];
    check(&m, [None, None, None]);
}

#[test]
fn single_call_goes_to_the_fastest_car() {
    let m = [[ok(30)], [ok(10)], [ok(20)]];
    check(&m, [None, Some(0), None]);
}

#[test]
fn global_optimum_beats_nearest_first() {
    // Жадный выбор взял бы самую дешёвую ячейку A-X (1) и отдал Y кабине B за 100: итого 101.
    // Глобально лучше A-Y и B-X: 2 + 2 = 4.
    let m = [[ok(1), ok(2)], [ok(2), ok(100)], [ok(100), ok(100)]];
    check(&m, [Some(1), Some(0), None]);
}

#[test]
fn forbidden_pair_is_never_chosen_even_with_zero_eta() {
    let m = [[no(0)], [ok(50)], [ok(70)]];
    check(&m, [None, Some(0), None]);
}

#[test]
fn all_forbidden_gives_nothing() {
    let m = [[no(0), no(0)], [no(0), no(0)], [no(0), no(0)]];
    check(&m, [None, None, None]);
}

#[test]
fn more_pairs_beat_lower_total_eta() {
    // Одна пара B-X за 1 дешевле, чем две A-X и B-Y за 1010, но пар должно быть больше.
    let m = [[ok(10), no(0)], [ok(1), ok(1000)], [no(0), no(0)]];
    check(&m, [Some(0), Some(1), None]);
}

#[test]
fn more_calls_than_cars_gives_each_car_at_most_one() {
    let m = [
        [ok(9), ok(1), ok(9), ok(9), ok(9)],
        [ok(9), ok(9), ok(2), ok(9), ok(9)],
        [ok(9), ok(9), ok(9), ok(3), ok(9)],
    ];
    check(&m, [Some(1), Some(2), Some(3)]);
}

#[test]
fn equal_eta_prefers_lower_car_id() {
    let m = [[ok(5)], [ok(5)], [ok(5)]];
    check(&m, [Some(0), None, None]);
}

#[test]
fn equal_eta_prefers_earlier_calls_for_lower_cars() {
    let m = [
        [ok(5), ok(5), ok(5)],
        [ok(5), ok(5), ok(5)],
        [ok(5), ok(5), ok(5)],
    ];
    check(&m, [Some(0), Some(1), Some(2)]);
}

#[test]
fn equal_eta_two_calls_three_cars_leaves_the_last_car_idle() {
    let m = [[ok(5), ok(5)], [ok(5), ok(5)], [ok(5), ok(5)]];
    check(&m, [Some(0), Some(1), None]);
}

#[test]
fn tie_in_total_prefers_lower_car_on_earlier_call() {
    // A-X + B-Y = 1 + 4 = 5 и A-Y + B-X = 2 + 3 = 5: сумма одна, побеждает A на первом вызове.
    let m = [[ok(1), ok(2)], [ok(3), ok(4)], [no(0), no(0)]];
    check(&m, [Some(0), Some(1), None]);
}

#[test]
fn tie_between_car_sets_prefers_lower_car_ids() {
    // Три назначения по 2 пары и сумме 2: (A, B), (A, C), (C, B). Берётся (A, B).
    let m = [[ok(1), ok(9)], [ok(9), ok(1)], [ok(1), ok(1)]];
    check(&m, [Some(0), Some(1), None]);
}

#[test]
fn eta_sums_do_not_overflow() {
    // Диагональ: MAX + MAX + 2 = 2^33, в u32 это 0, и она показалась бы самой дешёвой.
    // Настоящий оптимум: циклический сдвиг по 300 000 (всего 900 000).
    let max = u32::MAX;
    let m = [
        [ok(max), ok(300_000), ok(max)],
        [ok(max), ok(max), ok(300_000)],
        [ok(300_000), ok(max), ok(2)],
    ];
    check(&m, [Some(1), Some(2), Some(0)]);
}

#[test]
fn random_small_matrices_match_the_oracle() {
    fuzz::<3, 0>(1, 1);
    fuzz::<3, 1>(2, 500);
    fuzz::<3, 2>(3, 2000);
    fuzz::<3, 3>(4, 2000);
    fuzz::<3, 4>(5, 2000);
    fuzz::<3, 5>(6, 2000);
    fuzz::<2, 3>(7, 1000);
    fuzz::<1, 3>(8, 500);
}

#[test]
fn same_input_gives_same_output() {
    let m = [
        [ok(4), ok(4), no(1)],
        [ok(4), ok(4), ok(4)],
        [no(0), ok(4), ok(4)],
    ];
    assert_eq!(assign(&m), assign(&m));
}
