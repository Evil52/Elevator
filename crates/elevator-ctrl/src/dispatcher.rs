//! Диспетчер группы: связывает таблицу вызовов, состояние кабин, матрицу ETA и назначение.
//!
//! Это склейка готовых частей: [`CallTable`] хранит вызовы, [`build_matrix`] оценивает пары
//! «кабина и вызов», [`assign`](crate::assign) выбирает пары. Собственных правил у диспетчера
//! почти нет, он только решает, в каком порядке и по каким наблюдениям двигать вызовы по
//! состояниям. Время передаётся аргументом, ни кучи, ни паник.
//!
//! Вызывающий каждый шаг передаёт наблюдения за кабинами и получает события через
//! замыкание: так события не нужно складывать в буфер, а их порядок остаётся фиксированным.

// Заглушка: тела методов пустые, поэтому часть полей и `self` пока не используется.
// Атрибут уходит вместе с заглушками.
#![allow(clippy::unused_self, dead_code)]

use crate::calls::{CallId, CallTable, CarId, Press, Rejection, Served};
use crate::eta::{Call, CarState, EtaParams};

/// Что диспетчер знает о кабине на этом шаге.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarInput {
    /// Состояние для ETA. Поля `up_calls` и `down_calls` диспетчер игнорирует: он заполняет
    /// их сам из таблицы (вызовы в ASSIGNED, назначенные этой кабине). Поле `car_calls`
    /// (этажи, нажатые пассажирами внутри) берётся как есть.
    pub state: CarState,
    /// Этаж, на котором кабина стоит в зоне дверей и двери открываются или уже открыты.
    pub stopped_at: Option<u8>,
    /// Двери полностью открыты. Учитывается только вместе с `stopped_at`.
    pub doors_open: bool,
}

/// Событие шага. Поле `call` это вызов этажа, `id` его номер, `car` номер кабины.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// QUEUED стал ASSIGNED.
    Assigned { call: Call, id: CallId, car: CarId },
    /// ASSIGNED стал ARRIVED.
    Arrived { call: Call, id: CallId, car: CarId },
    /// ARRIVED стал SERVED, вызов ушёл из таблицы.
    Served { call: Call, served: Served },
    /// Кабина вышла из обычного режима, вызов вернулся в QUEUED. `car` это выбывшая кабина.
    Requeued { call: Call, id: CallId, car: CarId },
    /// Вызов отменён режимом «пожарная опасность».
    Cancelled { call: Call, id: CallId },
}

/// Диспетчер здания на `FLOORS` этажей и `CARS` кабин. За один проход назначения
/// рассматриваются не больше `CALLS` самых давних вызовов в QUEUED: остальные ждут
/// следующего шага.
#[derive(Clone, Debug)]
pub struct Dispatcher<const FLOORS: usize, const CARS: usize, const CALLS: usize> {
    table: CallTable<FLOORS>,
    params: EtaParams,
}

impl<const FLOORS: usize, const CARS: usize, const CALLS: usize> Dispatcher<FLOORS, CARS, CALLS> {
    /// Пустой диспетчер. `None`, если число этажей в `params.building` не равно `FLOORS`
    /// или кабин больше 256 (номер кабины это `u8`).
    #[must_use]
    pub fn new(params: EtaParams) -> Option<Self> {
        Some(Self {
            table: CallTable::new(),
            params,
        })
    }

    /// Нажатие кнопки этажа в момент `now_ms`: то же, что [`CallTable::press`].
    ///
    /// # Errors
    /// Этаж вне здания, вызов вниз с первого этажа или вверх с верхнего.
    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, Rejection> {
        self.table.press(call, now_ms)
    }

    /// Таблица вызовов, только для чтения.
    #[must_use]
    pub const fn calls(&self) -> &CallTable<FLOORS> {
        &self.table
    }

    /// Один шаг диспетчера в момент `now_ms` по наблюдениям `cars` (кабины по id).
    ///
    /// Фазы идут строго в таком порядке, события выдаются по ходу фаз:
    ///
    /// 1. Пожар. Если хотя бы одна кабина в `CarMode::FireRecall`, отменяются все вызовы в
    ///    любом состоянии (по `Cancelled` на вызов, по возрастанию номера), и шаг на этом
    ///    заканчивается: ничего не назначается. Вызовы, нажатые во время режима, отменяются
    ///    на ближайшем шаге.
    /// 2. Выбывшие кабины. Для каждой кабины (по id), которая не принимает назначений,
    ///    все её вызовы в ASSIGNED и ARRIVED возвращаются в QUEUED (`Requeued`), номер и
    ///    время регистрации прежние.
    /// 3. Прибытие и двери. Для каждой кабины (по id) с `stopped_at = Some(этаж)` её вызовы
    ///    на этом этаже, сначала «вверх», потом «вниз»: ASSIGNED становится ARRIVED
    ///    (`Arrived`), и если `doors_open`, сразу ARRIVED становится SERVED (`Served`).
    ///    Вызов, уже стоящий в ARRIVED, при открытых дверях становится SERVED.
    ///    Вызовы других кабин и других этажей не трогаются.
    /// 4. Назначение. Берутся до `CALLS` вызовов в QUEUED в порядке `(время регистрации,
    ///    номер)`, строится матрица ETA ([`build_matrix`](crate::eta::build_matrix)) и
    ///    решается [`assign`](crate::assign). Ожидание вызова `now_ms - registered_ms`
    ///    (не меньше нуля, не больше `u32::MAX`). Каждая выбранная пара: `Assigned`, по
    ///    возрастанию id кабины. Кабина получает не больше одного нового вызова за шаг.
    ///    Недостающие до `CALLS` столбцы матрицы запрещены.
    ///
    /// Вызов, назначенный в фазе 4, в фазе 3 обрабатывается только на следующем шаге.
    /// Обслуженный вызов (SERVED) в очередь уже не возвращается.
    pub fn step(&mut self, now_ms: u64, cars: &[CarInput; CARS], on_event: impl FnMut(Event)) {
        let _ = (now_ms, cars, on_event, &self.params);
    }
}
