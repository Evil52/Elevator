//! Кинематика кабины для расчёта ETA: целые числа, без `f32`/`f64`.
//!
//! Движение с ограничением скорости, ускорения и рывка (S-кривая). Разгон от скорости
//! `v1` до `v2` идёт так: рывок растит ускорение до предела, ускорение держится, рывок
//! его снимает. Если до предела ускорение не успевает вырасти, остаются две фазы рывка.
//! Торможение зеркально разгону. Между разгоном и торможением кабина идёт на постоянной
//! скорости, которую выбираем как можно выше (но не выше максимальной), чтобы весь путь
//! уложился.
//!
//! Единицы снаружи: путь в мм, скорость в мм/с, ускорение в мм/с², рывок в мм/с³, время в
//! мс. Внутри всё в микрометрах, чтобы округления не накапливались на коротких перегонах.
//! Скорость на входе считается установившейся (ускорение в этот момент нулевое).

const UM: u128 = 1000;

/// Параметры движения кабины.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionParams {
    max_speed_mm_s: u32,
    accel_mm_s2: u32,
    jerk_mm_s3: u32,
}

impl MotionParams {
    /// Параметры движения. `None`, если скорость, ускорение или рывок равны нулю.
    #[must_use]
    pub const fn new(max_speed_mm_s: u32, accel_mm_s2: u32, jerk_mm_s3: u32) -> Option<Self> {
        if max_speed_mm_s == 0 || accel_mm_s2 == 0 || jerk_mm_s3 == 0 {
            return None;
        }
        Some(Self {
            max_speed_mm_s,
            accel_mm_s2,
            jerk_mm_s3,
        })
    }

    /// Максимальная скорость, мм/с.
    #[must_use]
    pub const fn max_speed_mm_s(&self) -> u32 {
        self.max_speed_mm_s
    }

    /// Путь, который нужен, чтобы остановиться с заданной скорости, в мм, если ускорение
    /// в этот момент нулевое. Округляется вверх: недооценить тормозной путь опаснее, чем
    /// переоценить.
    #[must_use]
    pub fn braking_distance_mm(&self, speed_mm_s: u32) -> u32 {
        let (_, micrometres) = self.ramp(0, u128::from(speed_mm_s) * UM);
        saturate(micrometres.div_ceil(UM))
    }

    /// Наименьший путь до остановки для кабины, которая идёт со скоростью `speed_mm_s` и
    /// разгоняется с ускорением `accel_mm_s2`. Сначала рывок гасит ускорение (скорость ещё
    /// немного растёт), потом обычное торможение. `None`, если такое состояние невозможно:
    /// скорость или ускорение выше предела, либо скорость после гашения ускорения выше
    /// максимальной. Торможение кабины в этой модели не описывается: тормозящую кабину
    /// считают с нулевым ускорением, это даёт запас по пути.
    #[must_use]
    pub fn stopping_distance_mm(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<u32> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let (_, brake_um) = self.ramp(0, settle.speed);
        Some(saturate((settle.path + brake_um).div_ceil(UM)))
    }

    /// Время, за которое кабина проедет `distance_mm` и остановится ровно в конце пути,
    /// если сейчас она идёт со скоростью `speed_mm_s` с нулевым ускорением. Миллисекунды,
    /// округление вверх по каждому участку профиля, поэтому оценка выше идеальной не
    /// больше чем на несколько миллисекунд.
    ///
    /// `None`, если остановиться точно в цели нельзя: скорость больше максимальной или
    /// тормозной путь длиннее `distance_mm`. Результат, не влезающий в `u32`, обрезается
    /// до `u32::MAX`.
    #[must_use]
    pub fn time_to_stop_ms(&self, distance_mm: u32, speed_mm_s: u32) -> Option<u32> {
        self.time_to_stop_from_ms(distance_mm, speed_mm_s, 0)
    }

    /// То же, но кабина сейчас разгоняется с ускорением `accel_mm_s2`: сперва рывок гасит
    /// ускорение (план, при котором кабина не пытается разгоняться дальше), потом
    /// профиль из [`Self::time_to_stop_ms`]. Это допустимый план, поэтому оценка
    /// не ниже идеальной и чуть выше неё.
    #[must_use]
    pub fn time_to_stop_from_ms(
        &self,
        distance_mm: u32,
        speed_mm_s: u32,
        accel_mm_s2: u32,
    ) -> Option<u32> {
        let settle = self.settle(speed_mm_s, accel_mm_s2)?;
        let rest = (u128::from(distance_mm) * UM).checked_sub(settle.path)?;
        Some(saturate(
            settle.millis + self.stop_time_um(rest, settle.speed)?,
        ))
    }

    // Состояние после гашения ускорения рывком вниз: время в мс, пройденный путь в мкм и
    // скорость в мкм/с, всё округлено вверх. `None`, если состояние невозможно.
    fn settle(&self, speed_mm_s: u32, accel_mm_s2: u32) -> Option<Settled> {
        if speed_mm_s > self.max_speed_mm_s || accel_mm_s2 > self.accel_mm_s2 {
            return None;
        }
        let speed = u128::from(speed_mm_s) * UM;
        let accel = u128::from(accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        let settled = Settled {
            millis: (1000 * accel).div_ceil(jerk),
            // Путь за время a/j при падающем с j ускорении: v*a/j + a³/(3*j²).
            path: (3 * speed * accel * jerk + accel * accel * accel).div_ceil(3 * jerk * jerk),
            speed: speed + (accel * accel).div_ceil(2 * jerk),
        };
        (settled.speed <= u128::from(self.max_speed_mm_s) * UM).then_some(settled)
    }

    // Время до остановки в мс для пути в мкм и скорости в мкм/с с нулевым ускорением.
    fn stop_time_um(&self, distance: u128, start: u128) -> Option<u128> {
        if self.ramp(0, start).1 > distance {
            return None;
        }
        if distance == 0 {
            return Some(0);
        }

        // Наибольшая скорость хода, при которой разгон до неё и торможение с неё помещаются.
        let (mut low, mut high) = (start, u128::from(self.max_speed_mm_s) * UM);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if self.profile(start, mid).1 <= distance {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let cruise = low.max(1);
        let (ramps_ms, covered) = self.profile(start, cruise);
        let cruise_ms = (1000 * distance.saturating_sub(covered)).div_ceil(cruise);
        Some(ramps_ms + cruise_ms)
    }

    // Разгон от `start` до `cruise` и торможение с `cruise` до остановки: время в мс и путь
    // в мкм, оба округлены вверх.
    fn profile(&self, start: u128, cruise: u128) -> (u128, u128) {
        let (accel_ms, accel_um) = self.ramp(start, cruise);
        let (brake_ms, brake_um) = self.ramp(0, cruise);
        (accel_ms + brake_ms, accel_um + brake_um)
    }

    // Разгон от скорости `from` до `to` (мкм/с, `from <= to`): время в мс и путь в мкм,
    // округлены вверх. Среднее значение скорости у симметричной S-кривой равно полусумме
    // начальной и конечной, поэтому путь это полусумма на время.
    fn ramp(&self, from: u128, to: u128) -> (u128, u128) {
        let delta = to - from;
        let sum = from + to;
        let accel = u128::from(self.accel_mm_s2) * UM;
        let jerk = u128::from(self.jerk_mm_s3) * UM;
        if delta * jerk >= accel * accel {
            // Три фазы: время дельта/a + a/j.
            let numerator = delta * jerk + accel * accel;
            (
                (1000 * numerator).div_ceil(accel * jerk),
                (sum * numerator).div_ceil(2 * accel * jerk),
            )
        } else {
            // Две фазы рывка: время 2 * sqrt(дельта / j).
            (
                ceil_sqrt_ratio(4_000_000 * delta, jerk),
                ceil_sqrt_ratio(sum * sum * delta, jerk),
            )
        }
    }
}

// Результат гашения ускорения.
struct Settled {
    millis: u128,
    path: u128,
    speed: u128,
}

fn saturate(value: u128) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

// Наименьшее целое s, при котором s² >= n.
fn ceil_sqrt(n: u128) -> u128 {
    let root = n.isqrt();
    if root * root < n { root + 1 } else { root }
}

// Наименьшее целое t, при котором t² * den >= num. Целое t² не меньше num / den ровно тогда,
// когда оно не меньше num / den, округлённого вверх.
fn ceil_sqrt_ratio(num: u128, den: u128) -> u128 {
    ceil_sqrt(num.div_ceil(den))
}
