use crate::calls::{
    Assignment, CallId, CallRecord, CallState, CallTable, CarId, Press, Rejection, Served,
};
use crate::eta::{Call, CarMode, CarState, Direction, EtaParams, QueuedCall, build_matrix};
use crate::{Cell, assign};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarInput {
    pub state: CarState,
    pub stopped_at: Option<u8>,
    pub doors_open: bool,
    /// Назначения, подтверждаемые кабиной на этой остановке (до двух направлений).
    pub confirmations: [Option<Assignment>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Assigned(Assignment),
    Arrived { call: Call, id: CallId, car: CarId },
    Served { call: Call, served: Served },
    Requeued { call: Call, id: CallId, car: CarId },
    Cancelled { call: Call, id: CallId },
}

#[derive(Clone, Debug)]
pub struct Dispatcher<const FLOORS: usize, const CARS: usize, const CALLS: usize> {
    table: CallTable<FLOORS>,
    params: EtaParams,
}

const PADDING: Call = Call {
    floor: 0,
    direction: Direction::Up,
};

fn car_id(index: usize) -> CarId {
    CarId::try_from(index).unwrap_or(CarId::MAX)
}

impl<const FLOORS: usize, const CARS: usize, const CALLS: usize> Dispatcher<FLOORS, CARS, CALLS> {
    #[must_use]
    pub fn new(params: EtaParams) -> Option<Self> {
        let fits =
            usize::from(params.building.floors()) == FLOORS && CARS <= usize::from(CarId::MAX) + 1;
        fits.then(|| Self {
            table: CallTable::new(),
            params,
        })
    }

    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, Rejection> {
        self.table.press(call, now_ms)
    }

    #[must_use]
    pub const fn calls(&self) -> &CallTable<FLOORS> {
        &self.table
    }

    pub fn step(&mut self, now_ms: u64, cars: &[CarInput; CARS], on_event: impl FnMut(Event)) {
        self.step_with_plan(now_ms, cars, on_event, |_, _, _| {});
    }

    /// Передаёт наблюдателю ту же матрицу и решение, из которых выдаются назначения.
    pub fn step_with_plan(
        &mut self,
        now_ms: u64,
        cars: &[CarInput; CARS],
        mut on_event: impl FnMut(Event),
        on_plan: impl FnOnce(&[QueuedCall], &[[Cell; CALLS]; CARS], &[Option<usize>; CARS]),
    ) {
        if cars.iter().any(|car| car.state.mode == CarMode::FireRecall) {
            self.cancel_everything(&mut on_event);
            return;
        }
        self.requeue_lost_cars(cars, &mut on_event);
        self.register_stops(cars, &mut on_event);
        self.assign_queued(now_ms, cars, &mut on_event, on_plan);
    }

    fn all_calls() -> impl Iterator<Item = Call> {
        (1..=u8::MAX).take(FLOORS).flat_map(|floor| {
            [Direction::Up, Direction::Down]
                .into_iter()
                .map(move |direction| Call { floor, direction })
        })
    }

    fn first_after(
        &self,
        after: Option<CallId>,
        pick: impl Fn(&CallRecord) -> bool,
    ) -> Option<(Call, CallRecord)> {
        Self::all_calls()
            .filter_map(|call| self.table.get(call).map(|record| (call, record)))
            .filter(|(_, record)| after.is_none_or(|last| record.id > last) && pick(record))
            .min_by_key(|(_, record)| record.id)
    }

    fn for_each_by_id(
        &self,
        pick: impl Fn(&CallRecord) -> bool,
        mut visit: impl FnMut(Call, CallRecord),
    ) {
        let mut last = None;
        while let Some((call, record)) = self.first_after(last, &pick) {
            visit(call, record);
            last = Some(record.id);
        }
    }

    fn cancel_everything(&mut self, on_event: &mut impl FnMut(Event)) {
        self.for_each_by_id(
            |_| true,
            |call, record| {
                on_event(Event::Cancelled {
                    call,
                    id: record.id,
                });
            },
        );
        self.table.cancel_all();
    }

    fn requeue_lost_cars(&mut self, cars: &[CarInput; CARS], on_event: &mut impl FnMut(Event)) {
        for (index, car) in cars.iter().enumerate() {
            if car.state.mode.accepts_assignments() {
                continue;
            }
            let lost = car_id(index);
            self.for_each_by_id(
                |record| record.car == Some(lost),
                |call, record| {
                    on_event(Event::Requeued {
                        call,
                        id: record.id,
                        car: lost,
                    });
                },
            );
            self.table.car_lost(lost);
        }
    }

    fn register_stops(&mut self, cars: &[CarInput; CARS], on_event: &mut impl FnMut(Event)) {
        for (index, car) in cars.iter().enumerate() {
            let Some(floor) = car.stopped_at else {
                continue;
            };
            let id = car_id(index);
            for ticket in car.confirmations.into_iter().flatten() {
                let call = ticket.call;
                if ticket.car != id || call.floor != floor {
                    continue;
                }
                if self.table.arrived(ticket).is_ok() {
                    on_event(Event::Arrived {
                        call,
                        id: ticket.id,
                        car: id,
                    });
                }
                if car.doors_open
                    && let Ok(served) = self.table.doors_opened(ticket)
                {
                    on_event(Event::Served { call, served });
                }
            }
        }
    }

    fn assigned_masks(&self, car: CarId) -> (u32, u32) {
        Self::all_calls().fold((0, 0), |(up, down), call| match self.table.get(call) {
            Some(record) if record.state == CallState::Assigned && record.car == Some(car) => {
                let bit = 1_u32
                    .checked_shl(u32::from(call.floor) - 1)
                    .unwrap_or_default();
                match call.direction {
                    Direction::Up => (up | bit, down),
                    Direction::Down => (up, down | bit),
                }
            }
            _ => (up, down),
        })
    }

    fn assign_queued(
        &mut self,
        now_ms: u64,
        cars: &[CarInput; CARS],
        on_event: &mut impl FnMut(Event),
        on_plan: impl FnOnce(&[QueuedCall], &[[Cell; CALLS]; CARS], &[Option<usize>; CARS]),
    ) {
        let mut queued = [PADDING; CALLS];
        let count = self.table.queued(&mut queued);
        if count == 0 {
            return;
        }
        let window: [QueuedCall; CALLS] =
            core::array::from_fn(|index| match queued.get(index).filter(|_| index < count) {
                Some(&call) => QueuedCall {
                    call,
                    waited_ms: self.table.get(call).map_or(0, |record| {
                        u32::try_from(now_ms.saturating_sub(record.registered_ms))
                            .unwrap_or(u32::MAX)
                    }),
                },
                None => QueuedCall {
                    call: PADDING,
                    waited_ms: 0,
                },
            });
        let states: [CarState; CARS] = core::array::from_fn(|index| {
            let (up_calls, down_calls) = self.assigned_masks(car_id(index));
            CarState {
                up_calls,
                down_calls,
                ..cars[index].state
            }
        });
        let matrix = build_matrix(&self.params, &states, &window);
        let picks = assign(&matrix);
        on_plan(&window[..count], &matrix, &picks);
        for (index, pick) in picks.iter().enumerate() {
            let Some(&call) = pick.and_then(|column| queued.get(column)) else {
                continue;
            };
            let car = car_id(index);
            if let Ok(ticket) = self.table.assign(call, car) {
                on_event(Event::Assigned(ticket));
            }
        }
    }
}
