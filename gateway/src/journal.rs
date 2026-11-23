use std::collections::{BTreeMap, VecDeque};
use std::io::Write;

use elevator_wire::protocol::{CarSnapshot, Envelope, Message};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Rx,
    Tx,
}

pub struct Journal {
    entries: VecDeque<String>,
    capacity: usize,
    dropped: u64,
    last_snapshots: BTreeMap<u8, CarSnapshot>,
    sink: Option<Box<dyn Write + Send>>,
    sink_errors: u64,
}

impl Journal {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            dropped: 0,
            last_snapshots: BTreeMap::new(),
            sink: None,
            sink_errors: 0,
        }
    }

    #[must_use]
    pub fn with_sink(mut self, sink: Box<dyn Write + Send>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn record(
        &mut self,
        now_ms: u64,
        direction: Direction,
        session: Option<u64>,
        envelope: &Envelope,
    ) -> bool {
        if self.capacity == 0 {
            self.dropped += 1;
            return false;
        }
        if let Message::CarSnapshot(snapshot) = &envelope.message {
            let mut shape = *snapshot;
            shape.model_time_ms = 0;
            if self.last_snapshots.get(&snapshot.car) == Some(&shape) {
                return false;
            }
            self.last_snapshots.insert(snapshot.car, shape);
        }
        let Ok(value) = serde_json::to_value(envelope) else {
            return false;
        };
        let direction = match direction {
            Direction::Rx => "rx",
            Direction::Tx => "tx",
        };
        let line = json!({"t": now_ms, "dir": direction, "session": session, "envelope": value});
        self.push(line.to_string());
        true
    }

    pub fn record_refusal(&mut self, now_ms: u64, session: u64, reason: &str) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        let line = json!({"t": now_ms, "dir": "refused", "session": session, "reason": reason});
        self.push(line.to_string());
    }

    pub fn record_link(&mut self, now_ms: u64, up: bool) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        let state = if up { "up" } else { "down" };
        let line = json!({"t": now_ms, "dir": "link", "state": state});
        self.push(line.to_string());
    }

    fn push(&mut self, line: String) {
        if let Some(sink) = self.sink.as_mut()
            && sink
                .write_all(line.as_bytes())
                .and_then(|()| sink.write_all(b"\n"))
                .and_then(|()| sink.flush())
                .is_err()
        {
            self.sink_errors += 1;
        }
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        self.entries.push_back(line);
    }

    #[must_use]
    pub fn export(&self, now_ms: u64) -> String {
        let header = json!({
            "t": now_ms,
            "journal": {"version": 1, "entries": self.entries.len(), "dropped": self.dropped}
        });
        let mut text = header.to_string();
        text.push('\n');
        for line in &self.entries {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    #[must_use]
    pub const fn sink_errors(&self) -> u64 {
        self.sink_errors
    }
}
