use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use elevator_wire::protocol::{Envelope, Message};

struct Inner {
    clients: Vec<Sender<String>>,
    cache: BTreeMap<String, String>,
}

pub struct Hub {
    inner: Mutex<Inner>,
}

#[must_use]
pub fn cache_key(envelope: &Envelope) -> Option<String> {
    match &envelope.message {
        Message::Hello(_) => Some("hello".to_owned()),
        Message::CarSnapshot(snapshot) => Some(format!("car:{}", snapshot.car)),
        Message::AlarmPage(page) => Some(format!("alarms:{}", page.offset)),
        _ => None,
    }
}

impl Hub {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                clients: Vec::new(),
                cache: BTreeMap::new(),
            }),
        }
    }

    pub fn publish(&self, envelope: &Envelope, json: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(key) = cache_key(envelope) {
            inner.cache.insert(key, json.to_owned());
        }
        inner
            .clients
            .retain(|client| client.send(json.to_owned()).is_ok());
    }

    #[must_use]
    pub fn subscribe(&self) -> (Vec<String>, Receiver<String>) {
        let (sender, receiver) = channel();
        let Ok(mut inner) = self.inner.lock() else {
            return (Vec::new(), receiver);
        };
        let replay = inner.cache.values().cloned().collect();
        inner.clients.push(sender);
        (replay, receiver)
    }

    #[must_use]
    pub fn clients(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.clients.len())
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
