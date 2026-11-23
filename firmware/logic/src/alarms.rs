use elevator_wire::protocol::{ALARMS_PER_PAGE, Alarm, AlarmKind, AlarmPage, AlarmState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raised {
    New(u32),
    Existing(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmError {
    NoSuchAlarm,
    WrongState,
    TableFull,
}

#[derive(Clone, Copy, Debug)]
pub struct AlarmTable<const N: usize> {
    items: [Option<Alarm>; N],
    next_id: u32,
}

impl<const N: usize> AlarmTable<N> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: [None; N],
            next_id: 1,
        }
    }

    pub fn raise(
        &mut self,
        kind: AlarmKind,
        car: Option<u8>,
        at_ms: u64,
    ) -> Result<Raised, AlarmError> {
        if let Some(open) = self.items.iter().flatten().find(|alarm| {
            alarm.kind == kind && alarm.car == car && alarm.state != AlarmState::Cleared
        }) {
            return Ok(Raised::Existing(open.id));
        }
        if self.next_id == u32::MAX {
            return Err(AlarmError::TableFull);
        }
        let slot = self.free_slot().ok_or(AlarmError::TableFull)?;
        let id = self.next_id;
        self.next_id += 1;
        if let Some(entry) = self.items.get_mut(slot) {
            *entry = Some(Alarm {
                id,
                kind,
                car,
                state: AlarmState::Active,
                raised_at_ms: at_ms,
            });
        }
        Ok(Raised::New(id))
    }

    pub fn acknowledge(&mut self, id: u32) -> Result<(), AlarmError> {
        let alarm = self
            .items
            .iter_mut()
            .flatten()
            .find(|alarm| alarm.id == id)
            .ok_or(AlarmError::NoSuchAlarm)?;
        if alarm.state != AlarmState::Active {
            return Err(AlarmError::WrongState);
        }
        alarm.state = AlarmState::Acknowledged;
        Ok(())
    }

    pub fn clear(&mut self, kind: AlarmKind, car: Option<u8>) -> Option<u32> {
        let alarm = self.items.iter_mut().flatten().find(|alarm| {
            alarm.kind == kind && alarm.car == car && alarm.state != AlarmState::Cleared
        })?;
        alarm.state = AlarmState::Cleared;
        Some(alarm.id)
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<Alarm> {
        self.items
            .iter()
            .flatten()
            .find(|alarm| alarm.id == id)
            .copied()
    }

    #[must_use]
    pub fn has_active(&self, kinds: &[AlarmKind]) -> bool {
        self.items
            .iter()
            .flatten()
            .any(|alarm| alarm.state == AlarmState::Active && kinds.contains(&alarm.kind))
    }

    #[must_use]
    pub fn total(&self) -> u16 {
        u16::try_from(self.items.iter().flatten().count()).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn page(&self, offset: u16) -> AlarmPage {
        let total = self.total();
        let offset = offset.min(total);
        let mut items = [None; ALARMS_PER_PAGE];
        let mut last = 0;
        for position in 0..usize::from(offset) + ALARMS_PER_PAGE {
            let Some(next) = self
                .items
                .iter()
                .flatten()
                .filter(|alarm| alarm.id > last)
                .min_by_key(|alarm| alarm.id)
            else {
                break;
            };
            last = next.id;
            if let Some(slot) = position
                .checked_sub(usize::from(offset))
                .and_then(|index| items.get_mut(index))
            {
                *slot = Some(*next);
            }
        }
        AlarmPage {
            total,
            offset,
            items,
        }
    }

    fn free_slot(&self) -> Option<usize> {
        if let Some(index) = self.items.iter().position(Option::is_none) {
            return Some(index);
        }
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.filter(|alarm| alarm.state == AlarmState::Cleared)
                    .map(|alarm| (alarm.id, index))
            })
            .min()
            .map(|(_, index)| index)
    }
}

impl<const N: usize> Default for AlarmTable<N> {
    fn default() -> Self {
        Self::new()
    }
}
