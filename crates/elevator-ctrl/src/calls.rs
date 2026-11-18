//! Автомат вызовов этажа: QUEUED, ASSIGNED, ARRIVED, SERVED.
//!
//! Вызов задаётся парой «этаж и направление кнопки», поэтому повторное нажатие той же кнопки
//! это тот же вызов, а память фиксирована и не требует кучи. Состояние SERVED наступает, когда
//! двери полностью открылись; после него вызов из таблицы уходит и результат возвращается
//! вызывающему. Время передаётся аргументом.
//!
//! Сейчас здесь контракт и заглушки: тесты из `tests/calls.rs` красные. Реализацию пишет
//! владелец крейта, поля структуры подсказка, их можно менять.
// Убрать вместе с заглушками.
#![allow(clippy::unused_self, dead_code)]

use crate::eta::Call;

/// Номер кабины (A, B, C это 0, 1, 2).
pub type CarId = u8;

/// Номер вызова. Растёт с каждым новым вызовом и не переиспользуется: по нему при равном
/// времени регистрации разрешается порядок.
pub type CallId = u32;

/// Состояние вызова в таблице. SERVED здесь нет: обслуженный вызов из таблицы уходит.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    /// Зарегистрирован, кабина не назначена.
    Queued,
    /// Назначена кабина, она едет.
    Assigned,
    /// Кабина остановилась на этаже вызова, двери ещё открываются.
    Arrived,
}

/// Вызов в таблице.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub id: CallId,
    pub state: CallState,
    /// Когда вызов зарегистрирован. При возврате в очередь не меняется.
    pub registered_ms: u64,
    /// Назначенная кабина: `Some` в ASSIGNED и ARRIVED, `None` в QUEUED.
    pub car: Option<CarId>,
}

/// Результат нажатия кнопки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Создан новый вызов.
    Created(CallId),
    /// Такой вызов уже есть (в любом состоянии), второй не создан и ничего не изменилось.
    Duplicate(CallId),
}

/// Почему вызов отклонён.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// Этажа нет в здании.
    NoSuchFloor,
    /// Вызов вниз с первого этажа.
    DownFromFirst,
    /// Вызов вверх с верхнего этажа.
    UpFromTop,
}

/// Почему переход не выполнен.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    /// Такого вызова в таблице нет.
    UnknownCall,
    /// Вызов есть, но в этом состоянии такой переход невозможен.
    WrongState,
}

/// Вызов, который обслужен: двери полностью открылись.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Served {
    pub id: CallId,
    pub car: CarId,
    pub registered_ms: u64,
}

/// Таблица вызовов здания на `FLOORS` этажей.
#[derive(Clone, Debug)]
pub struct CallTable<const FLOORS: usize> {
    up: [Option<CallRecord>; FLOORS],
    down: [Option<CallRecord>; FLOORS],
    next_id: CallId,
}

impl<const FLOORS: usize> CallTable<FLOORS> {
    /// Пустая таблица.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            up: [None; FLOORS],
            down: [None; FLOORS],
            next_id: 0,
        }
    }

    /// Нажатие кнопки в момент `now_ms`. Создаёт вызов в QUEUED. Неправильный вызов
    /// отклоняется с причиной, таблица и номера вызовов не меняются.
    ///
    /// # Errors
    /// Этаж вне здания, вызов вниз с первого этажа или вверх с верхнего.
    pub fn press(&mut self, _call: Call, _now_ms: u64) -> Result<Press, Rejection> {
        Err(Rejection::NoSuchFloor)
    }

    /// Вызов из таблицы, если он есть.
    #[must_use]
    pub fn get(&self, _call: Call) -> Option<CallRecord> {
        None
    }

    /// QUEUED переходит в ASSIGNED: вызов назначен кабине.
    ///
    /// # Errors
    /// Вызова нет или он не в QUEUED.
    pub fn assign(&mut self, _call: Call, _car: CarId) -> Result<(), TransitionError> {
        Err(TransitionError::UnknownCall)
    }

    /// ASSIGNED переходит в ARRIVED: кабина встала на этаже вызова.
    ///
    /// # Errors
    /// Вызова нет или он не в ASSIGNED.
    pub fn arrived(&mut self, _call: Call) -> Result<(), TransitionError> {
        Err(TransitionError::UnknownCall)
    }

    /// Двери полностью открылись: ARRIVED переходит в SERVED, вызов уходит из таблицы.
    ///
    /// # Errors
    /// Вызова нет или он не в ARRIVED.
    pub fn doors_opened(&mut self, _call: Call) -> Result<Served, TransitionError> {
        Err(TransitionError::UnknownCall)
    }

    /// Кабина выбыла: все её вызовы в ASSIGNED и ARRIVED сразу возвращаются в QUEUED с
    /// исходным временем регистрации и тем же номером. Возвращает, сколько вызовов вернулось.
    pub fn car_lost(&mut self, _car: CarId) -> usize {
        0
    }

    /// Отменяет все вызовы в любом состоянии (режим «пожарная опасность», ГОСТ Р 53296-2009).
    /// Возвращает, сколько вызовов отменено. Номера вызовов дальше продолжают расти.
    pub fn cancel_all(&mut self) -> usize {
        0
    }

    /// Кладёт вызовы в QUEUED в `out` по порядку: раньше зарегистрированный первым, при равном
    /// времени меньший номер. Пишет не больше `out.len()` первых вызовов, возвращает сколько
    /// записано.
    pub fn queued(&self, _out: &mut [Call]) -> usize {
        0
    }
}

impl<const FLOORS: usize> Default for CallTable<FLOORS> {
    fn default() -> Self {
        Self::new()
    }
}
