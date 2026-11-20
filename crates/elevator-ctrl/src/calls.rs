use crate::eta::{Call, Direction};

pub type CarId = u8;

pub type CallId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub call: Call,
    pub id: CallId,
    pub car: CarId,
    pub attempt: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    Queued,
    Assigned,
    Arrived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub id: CallId,
    pub state: CallState,
    pub registered_ms: u64,
    pub car: Option<CarId>,
    pub attempt: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Created(CallId),
    Duplicate(CallId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    NoSuchFloor,
    DownFromFirst,
    UpFromTop,
    IdExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    UnknownCall,
    WrongState,
    StaleAssignment,
    AttemptsExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Served {
    pub id: CallId,
    pub car: CarId,
    pub registered_ms: u64,
}

#[derive(Clone, Debug)]
pub struct CallTable<const FLOORS: usize> {
    up: [Option<CallRecord>; FLOORS],
    down: [Option<CallRecord>; FLOORS],
    next_id: Option<CallId>,
}

impl<const FLOORS: usize> CallTable<FLOORS> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            up: [None; FLOORS],
            down: [None; FLOORS],
            next_id: Some(0),
        }
    }

    pub fn press(&mut self, call: Call, now_ms: u64) -> Result<Press, Rejection> {
        let index = Self::check(call)?;
        let slot = match call.direction {
            Direction::Up => &mut self.up[index],
            Direction::Down => &mut self.down[index],
        };
        if let Some(record) = slot {
            return Ok(Press::Duplicate(record.id));
        }
        let id = self.next_id.ok_or(Rejection::IdExhausted)?;
        self.next_id = id.checked_add(1);
        *slot = Some(CallRecord {
            id,
            state: CallState::Queued,
            registered_ms: now_ms,
            car: None,
            attempt: 0,
        });
        Ok(Press::Created(id))
    }

    #[must_use]
    pub fn get(&self, call: Call) -> Option<CallRecord> {
        self.slot(call).copied().flatten()
    }

    pub fn assign(&mut self, call: Call, car: CarId) -> Result<Assignment, TransitionError> {
        let record = self.record_in_state(call, CallState::Queued)?;
        record.attempt = record
            .attempt
            .checked_add(1)
            .ok_or(TransitionError::AttemptsExhausted)?;
        record.state = CallState::Assigned;
        record.car = Some(car);
        Ok(Assignment {
            call,
            id: record.id,
            car,
            attempt: record.attempt,
        })
    }

    fn assigned_slot(
        &mut self,
        assignment: Assignment,
        state: CallState,
    ) -> Result<(&mut Option<CallRecord>, CallRecord), TransitionError> {
        let slot = self
            .slot_mut(assignment.call)
            .ok_or(TransitionError::UnknownCall)?;
        let record = slot.ok_or(TransitionError::UnknownCall)?;
        if record.state != state {
            return Err(TransitionError::WrongState);
        }
        if record.id != assignment.id
            || record.car != Some(assignment.car)
            || record.attempt != assignment.attempt
        {
            return Err(TransitionError::StaleAssignment);
        }
        Ok((slot, record))
    }

    pub fn arrived(&mut self, assignment: Assignment) -> Result<(), TransitionError> {
        let (slot, record) = self.assigned_slot(assignment, CallState::Assigned)?;
        *slot = Some(CallRecord {
            state: CallState::Arrived,
            ..record
        });
        Ok(())
    }

    pub fn doors_opened(&mut self, assignment: Assignment) -> Result<Served, TransitionError> {
        let (slot, record) = self.assigned_slot(assignment, CallState::Arrived)?;
        *slot = None;
        Ok(Served {
            id: record.id,
            car: assignment.car,
            registered_ms: record.registered_ms,
        })
    }

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

    pub fn cancel_all(&mut self) -> usize {
        let mut cancelled = 0;
        for slot in self.up.iter_mut().chain(&mut self.down) {
            if slot.take().is_some() {
                cancelled += 1;
            }
        }
        cancelled
    }

    pub fn queued(&self, out: &mut [Call]) -> usize {
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
