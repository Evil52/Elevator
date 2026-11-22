use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{Error, Message, accept_hdr};

use crate::hub::Hub;
use crate::origin::OriginPolicy;
use crate::{Bridge, Stats, to_json};

const READ_SLICE: Duration = Duration::from_millis(50);

pub struct Gateway {
    bridge: Mutex<Bridge>,
    hub: Hub,
    writer: Mutex<Box<dyn Write + Send>>,
    policy: OriginPolicy,
    refused: AtomicU64,
}

impl Gateway {
    #[must_use]
    pub fn new(writer: Box<dyn Write + Send>, policy: OriginPolicy) -> Arc<Self> {
        Arc::new(Self {
            bridge: Mutex::new(Bridge::new()),
            hub: Hub::new(),
            writer: Mutex::new(writer),
            policy,
            refused: AtomicU64::new(0),
        })
    }

    pub fn ingest(&self, bytes: &[u8]) -> Vec<String> {
        let envelopes = match self.bridge.lock() {
            Ok(mut bridge) => bridge.feed(bytes),
            Err(_) => return Vec::new(),
        };
        let mut lines = Vec::new();
        for envelope in &envelopes {
            if let Ok(line) = to_json(envelope) {
                self.hub.publish(envelope, &line);
                lines.push(line);
            }
        }
        lines
    }

    pub fn command(&self, json: &str) -> bool {
        let frame = match self.bridge.lock() {
            Ok(mut bridge) => bridge.command_frame(json),
            Err(_) => return false,
        };
        let sent = frame.is_ok_and(|bytes| {
            self.writer.lock().is_ok_and(|mut writer| {
                writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_ok()
            })
        });
        if !sent {
            self.refused.fetch_add(1, Ordering::Relaxed);
        }
        sent
    }

    pub fn serve(self: &Arc<Self>, listener: &TcpListener) {
        for stream in listener.incoming().flatten() {
            let gateway = Arc::clone(self);
            thread::spawn(move || gateway.client(stream));
        }
    }

    #[must_use]
    pub fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn stats(&self) -> Stats {
        self.bridge
            .lock()
            .map_or_else(|_| Stats::default(), |bridge| bridge.stats())
    }

    #[must_use]
    pub fn clients(&self) -> usize {
        self.hub.clients()
    }

    fn origin_allowed(&self, request: &Request) -> bool {
        match request.headers().get("Origin") {
            None => self.policy.allows(None),
            Some(value) => value
                .to_str()
                .is_ok_and(|origin| self.policy.allows(Some(origin))),
        }
    }

    #[allow(clippy::result_large_err)]
    fn client(&self, stream: TcpStream) {
        let callback = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
            if self.origin_allowed(request) {
                Ok(response)
            } else {
                let mut refusal = ErrorResponse::new(Some("origin not allowed".to_owned()));
                *refusal.status_mut() = StatusCode::FORBIDDEN;
                Err(refusal)
            }
        };
        let Ok(mut socket) = accept_hdr(stream, callback) else {
            return;
        };
        if socket.get_mut().set_read_timeout(Some(READ_SLICE)).is_err() {
            return;
        }
        let (replay, live) = self.hub.subscribe();
        for line in replay {
            if socket.send(Message::text(line)).is_err() {
                return;
            }
        }
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    self.command(text.as_str());
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            while let Ok(line) = live.try_recv() {
                if socket.send(Message::text(line)).is_err() {
                    return;
                }
            }
        }
        let _ = socket.close(None);
    }
}
