//! Кинематика кабины для расчёта ETA: целые числа, без `f32`/`f64`.
//!
//! Единицы: путь в миллиметрах, скорость в мм/с, ускорение в мм/с², время в миллисекундах.
//! Профиль упрощённый: разгон и торможение с постоянным ускорением, между ними ход на
//! максимальной скорости. Рывок не учитывается. Параметры примерные, их ещё утвердят.

/// Параметры движения кабины.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionParams {
    max_speed_mm_s: u32,
    accel_mm_s2: u32,
}

impl MotionParams {
    /// Параметры движения. `None`, если скорость или ускорение равны нулю.
    #[must_use]
    pub const fn new(max_speed_mm_s: u32, accel_mm_s2: u32) -> Option<Self> {
        if max_speed_mm_s == 0 || accel_mm_s2 == 0 {
            return None;
        }
        Some(Self {
            max_speed_mm_s,
            accel_mm_s2,
        })
    }

    /// Максимальная скорость, мм/с.
    #[must_use]
    pub const fn max_speed_mm_s(&self) -> u32 {
        self.max_speed_mm_s
    }

    /// Путь, который нужен, чтобы остановиться с заданной скорости, в мм.
    /// Округляется вверх: недооценить тормозной путь опаснее, чем переоценить.
    #[must_use]
    pub fn braking_distance_mm(&self, speed_mm_s: u32) -> u32 {
        let speed = u128::from(speed_mm_s);
        let accel = u128::from(self.accel_mm_s2);
        saturate((speed * speed).div_ceil(2 * accel))
    }

    /// Время, за которое кабина проедет `distance_mm` и остановится ровно в конце пути,
    /// если сейчас она идёт со скоростью `speed_mm_s` в сторону цели. Миллисекунды,
    /// округление вверх.
    ///
    /// `None`, если остановиться точно в цели нельзя: скорость больше максимальной или
    /// тормозной путь длиннее `distance_mm`. Результат, не влезающий в `u32`, обрезается
    /// до `u32::MAX`.
    #[must_use]
    pub fn time_to_stop_ms(&self, distance_mm: u32, speed_mm_s: u32) -> Option<u32> {
        if speed_mm_s > self.max_speed_mm_s || distance_mm < self.braking_distance_mm(speed_mm_s) {
            return None;
        }
        let top = u128::from(self.max_speed_mm_s);
        let accel = u128::from(self.accel_mm_s2);
        let dist = u128::from(distance_mm);
        let speed = u128::from(speed_mm_s);

        // Разгон до `top`, ход и торможение укладываются в путь, если 2·a·d + v0² >= 2·top².
        let millis = if 2 * accel * dist + speed * speed >= 2 * top * top {
            // t = 1000 · (2·top² - 2·top·v0 + v0² + 2·a·d) / (2·a·top)
            let numerator =
                1000 * (2 * top * top + speed * speed + 2 * accel * dist - 2 * top * speed);
            numerator.div_ceil(2 * accel * top)
        } else {
            // Пика `top` не достигаем: пик p = sqrt(a·d + v0²/2), t = 1000 · (2p - v0) / a.
            // Берём наименьшее целое T, при котором (T·a + 1000·v0)² >= 10^6 · (4·a·d + 2·v0²).
            let squared = 1_000_000 * (4 * accel * dist + 2 * speed * speed);
            ceil_sqrt(squared)
                .saturating_sub(1000 * speed)
                .div_ceil(accel)
        };
        Some(saturate(millis))
    }
}

fn saturate(value: u128) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

// Наименьшее целое s, при котором s² >= n.
fn ceil_sqrt(n: u128) -> u128 {
    let root = n.isqrt();
    if root * root < n { root + 1 } else { root }
}
