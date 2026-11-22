use elevator_wire::protocol::{EVENTS_PER_CHUNK, Event, EventKind, EventLogChunk};

#[derive(Clone, Copy, Debug)]
pub struct EventLog<const N: usize> {
    items: [Option<Event>; N],
    next_index: u32,
}

impl<const N: usize> EventLog<N> {
    #[must_use]
    pub const fn new() -> Self {
        const { assert!(N >= 1) };
        Self {
            items: [None; N],
            next_index: 0,
        }
    }

    pub fn push(&mut self, at_ms: u64, kind: EventKind, car: Option<u8>, id: u32) -> bool {
        if self.next_index == u32::MAX {
            return false;
        }
        let index = self.next_index;
        let Some(slot) = self.items.get_mut(index as usize % N) else {
            return false;
        };
        let overwrote = slot.is_some();
        *slot = Some(Event {
            index,
            at_ms,
            kind,
            car,
            id,
        });
        self.next_index += 1;
        overwrote
    }

    #[must_use]
    pub const fn next_index(&self) -> u32 {
        self.next_index
    }

    #[must_use]
    pub fn oldest_index(&self) -> u32 {
        self.next_index
            .saturating_sub(u32::try_from(N).unwrap_or(u32::MAX))
    }

    #[must_use]
    pub fn chunk(&self, from_index: u32) -> EventLogChunk {
        let oldest = self.oldest_index();
        let first_index = from_index.max(oldest).min(self.next_index);
        let events = core::array::from_fn::<_, EVENTS_PER_CHUNK, _>(|offset| {
            let index = first_index.checked_add(u32::try_from(offset).ok()?)?;
            if index >= self.next_index {
                return None;
            }
            self.items.get(index as usize % N).copied().flatten()
        });
        EventLogChunk {
            first_index,
            dropped_before: oldest.saturating_sub(from_index),
            events,
        }
    }
}

impl<const N: usize> Default for EventLog<N> {
    fn default() -> Self {
        Self::new()
    }
}
