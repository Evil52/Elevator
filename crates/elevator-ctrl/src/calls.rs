//! Автомат вызовов этажа: QUEUED, ASSIGNED, ARRIVED, SERVED.
//!
//! Вызов задаётся парой «этаж и направление кнопки», поэтому повторное нажатие той же кнопки
//! это тот же вызов, а память фиксирована и не требует кучи. Состояние SERVED наступает, когда
//! двери полностью открылись; после него вызов из таблицы уходит и результат возвращается
//! вызывающему. Время передаётся аргументом.
//!
//! Все операции работают на массивах фиксированного размера: ни кучи, ни паник на неверных
//! входных данных. Порядок очереди даёт пара `(registered_ms, id)`, а не место в таблице.

use crate::eta::{Call, Direction};

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
    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, Rejection> {
        let index = Self::check(call)?;
        let slot = match call.direction {
            Direction::Up => &mut self.up[index],
            Direction::Down => &mut self.down[index],
        };
        if let Some(record) = slot {
            return Ok(Press::Duplicate(record.id));
        }
        let id = self.next_id;
        // Номер не переиспользуется: чтобы он вернулся к нулю, нужно 2^32 нажатий.
        self.next_id = id.wrapping_add(1);
        *slot = Some(CallRecord {
            id,
            state: CallState::Queued,
            registered_ms: now_ms,
            car: None,
        });
        Ok(Press::Created(id))
    }

    /// Вызов из таблицы, если он есть.
    #[must_use]
    pub fn get(&self, call: Call) -> Option<CallRecord> {
        self.slot(call).copied().flatten()
    }

    /// QUEUED переходит в ASSIGNED: вызов назначен кабине.
    ///
    /// # Errors
    /// Вызова нет или он не в QUEUED.
    pub fn assign(&mut self, call: Call, car: CarId) -> Result<(), TransitionError> {
        let record = self.record_in_state(call, CallState::Queued)?;
        record.state = CallState::Assigned;
        record.car = Some(car);
        Ok(())
    }

    /// ASSIGNED переходит в ARRIVED: кабина встала на этаже вызова.
    ///
    /// # Errors
    /// Вызова нет или он не в ASSIGNED.
    pub fn arrived(&mut self, call: Call) -> Result<(), TransitionError> {
        self.record_in_state(call, CallState::Assigned)?.state = CallState::Arrived;
        Ok(())
    }

    /// Двери полностью открылись: ARRIVED переходит в SERVED, вызов уходит из таблицы.
    ///
    /// # Errors
    /// Вызова нет или он не в ARRIVED.
    pub fn doors_opened(&mut self, call: Call) -> Result<Served, TransitionError> {
        let slot = self.slot_mut(call).ok_or(TransitionError::UnknownCall)?;
        match *slot {
            None => Err(TransitionError::UnknownCall),
            // В ARRIVED кабина всегда назначена, поэтому `car: Some` не сужает случай.
            Some(CallRecord {
                id,
                state: CallState::Arrived,
                registered_ms,
                car: Some(car),
            }) => {
                *slot = None;
                Ok(Served {
                    id,
                    car,
                    registered_ms,
                })
            }
            Some(_) => Err(TransitionError::WrongState),
        }
    }

    /// Кабина выбыла: все её вызовы в ASSIGNED и ARRIVED сразу возвращаются в QUEUED с
    /// исходным временем регистрации и тем же номером. Возвращает, сколько вызовов вернулось.
    pub fn car_lost(&mut self, car: CarId) -> usize {
        let mut returned = 0;
        for record in self.up.iter_mut().chain(&mut self.down).flatten() {
            if record.car == Some(car) {
                record.state = CallState::Queued;
                record.car = None;
                returned += 1;
            }
        }
        returned
    }

    /// Отменяет все вызовы в любом состоянии (режим «пожарная опасность», ГОСТ Р 53296-2009).
    /// Возвращает, сколько вызовов отменено. Номера вызовов дальше продолжают расти.
    pub fn cancel_all(&mut self) -> usize {
        let mut cancelled = 0;
        for slot in self.up.iter_mut().chain(&mut self.down) {
            if slot.take().is_some() {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Кладёт вызовы в QUEUED в `out` по порядку: раньше зарегистрированный первым, при равном
    /// времени меньший номер. Пишет не больше `out.len()` первых вызовов, возвращает сколько
    /// записано.
    pub fn queued(&self, out: &mut [Call]) -> usize {
        // Выборка без доп. памяти: каждый раз берём наименьший ключ больше предыдущего.
        // Ключи `(registered_ms, id)` не повторяются, поэтому строгого сравнения хватает.
        // Таблица маленькая (до 2 * FLOORS записей), лишние проходы ничего не стоят.
        let mut previous = None;
        let mut written = 0;
        for place in out.iter_mut() {
            let next = self
                .queued_entries()
                .filter(|(key, _)| previous.is_none_or(|last| *key > last))
                .min_by_key(|(key, _)| *key);
            let Some((key, call)) = next else { break };
            *place = call;
            previous = Some(key);
            written += 1;
        }
        written
    }

    // Проверка нажатия: индекс слота (этаж с нуля) или причина отказа.
    fn check(call: Call) -> Result<usize, Rejection> {
        let floor = usize::from(call.floor);
        if floor == 0 || floor > FLOORS {
            return Err(Rejection::NoSuchFloor);
        }
        match call.direction {
            Direction::Down if floor == 1 => Err(Rejection::DownFromFirst),
            Direction::Up if floor == FLOORS => Err(Rejection::UpFromTop),
            _ => Ok(floor - 1),
        }
    }

    // Слот вызова. `None`, если такого этажа нет.
    fn slot(&self, call: Call) -> Option<&Option<CallRecord>> {
        let index = usize::from(call.floor).checked_sub(1)?;
        match call.direction {
            Direction::Up => self.up.get(index),
            Direction::Down => self.down.get(index),
        }
    }

    fn slot_mut(&mut self, call: Call) -> Option<&mut Option<CallRecord>> {
        let index = usize::from(call.floor).checked_sub(1)?;
        match call.direction {
            Direction::Up => self.up.get_mut(index),
            Direction::Down => self.down.get_mut(index),
        }
    }

    // Запись вызова, который сейчас в состоянии `state`.
    fn record_in_state(
        &mut self,
        call: Call,
        state: CallState,
    ) -> Result<&mut CallRecord, TransitionError> {
        let record = self
            .slot_mut(call)
            .and_then(Option::as_mut)
            .ok_or(TransitionError::UnknownCall)?;
        if record.state == state {
            Ok(record)
        } else {
            Err(TransitionError::WrongState)
        }
    }

    // Вызовы в QUEUED вместе с ключом порядка `(registered_ms, id)`.
    fn queued_entries(&self) -> impl Iterator<Item = ((u64, CallId), Call)> + '_ {
        [(Direction::Up, &self.up), (Direction::Down, &self.down)]
            .into_iter()
            .flat_map(|(direction, slots)| {
                slots.iter().enumerate().filter_map(move |(index, slot)| {
                    let record = slot.as_ref().filter(|r| r.state == CallState::Queued)?;
                    let floor = u8::try_from(index + 1).ok()?;
                    Some(((record.registered_ms, record.id), Call { floor, direction }))
                })
            })
    }
}

impl<const FLOORS: usize> Default for CallTable<FLOORS> {
    fn default() -> Self {
        Self::new()
    }
}
