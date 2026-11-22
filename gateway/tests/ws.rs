use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use elevator_gateway::{Gateway, OriginPolicy, to_json};
use elevator_wire::protocol::{
    CallFloor, CarMode, CarSnapshot, Direction, DoorState, Envelope, Message, decode_message,
};
use elevator_wire::{MAX_FRAME, MAX_PAYLOAD, decode_frame, encode_frame};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{WebSocket, connect};

const ORIGIN: &str = "http://localhost:5173";

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn start(missing_origin_ok: bool) -> (Arc<Gateway>, SocketAddr, Sink) {
    let sink = Sink::default();
    let gateway = Gateway::new(
        Box::new(sink.clone()),
        OriginPolicy::new(vec![ORIGIN.to_owned()], missing_origin_ok),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = Arc::clone(&gateway);
    thread::spawn(move || server.serve(&listener));
    (gateway, addr, sink)
}

fn connect_with(
    addr: SocketAddr,
    origin: Option<&str>,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, tungstenite::Error> {
    let mut request = format!("ws://{addr}")
        .into_client_request()
        .expect("request");
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", origin.parse().expect("header"));
    }
    connect(request).map(|(socket, _)| socket)
}

fn set_timeout(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("timeout");
    }
}

fn next_text(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, wait: Duration) -> Option<String> {
    let end = Instant::now() + wait;
    while Instant::now() < end {
        match socket.read() {
            Ok(message) if message.is_text() => {
                return message.into_text().ok().map(|t| t.to_string());
            }
            _ => {}
        }
    }
    None
}

fn board_frame(envelope: &Envelope) -> Vec<u8> {
    let mut payload = [0_u8; MAX_PAYLOAD];
    let length = elevator_wire::protocol::encode_message(envelope, &mut payload).expect("message");
    let mut out = [0_u8; MAX_FRAME];
    let written = encode_frame(0, &payload[..length], &mut out).expect("frame");
    out[..written].to_vec()
}

fn snapshot(time: u64) -> Envelope {
    Envelope::new(
        4,
        1,
        1,
        Message::CarSnapshot(CarSnapshot {
            car: 0,
            mode: CarMode::Normal,
            doors: DoorState::Closed,
            floor: Some(1),
            position_mm: 0,
            speed_mm_per_sec: 0,
            eta_ms: None,
            up_calls: 0,
            down_calls: 0,
            model_time_ms: time,
        }),
    )
}

fn call_json() -> String {
    to_json(&Envelope::new(
        4,
        1,
        9,
        Message::CallFloor(CallFloor {
            request_id: 5,
            floor: 3,
            direction: Direction::Down,
        }),
    ))
    .expect("json")
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_client_with_a_listed_origin_receives_board_telemetry_as_json() {
    let (gateway, addr, _) = start(false);
    let mut client = connect_with(addr, Some(ORIGIN)).expect("connect");
    set_timeout(&mut client);
    assert!(wait_until(|| gateway.clients() == 1));
    let lines = gateway.ingest(&board_frame(&snapshot(77)));
    assert_eq!(lines.len(), 1);
    let text = next_text(&mut client, Duration::from_secs(3)).expect("telemetry");
    assert_eq!(text, lines[0]);
    assert!(text.contains("\"modelTimeMs\":77"));
}

#[test]
fn a_late_client_gets_the_latest_state_immediately() {
    let (gateway, addr, _) = start(false);
    gateway.ingest(&board_frame(&snapshot(1)));
    gateway.ingest(&board_frame(&snapshot(2)));
    let mut client = connect_with(addr, Some(ORIGIN)).expect("connect");
    set_timeout(&mut client);
    let text = next_text(&mut client, Duration::from_secs(3)).expect("replay");
    assert!(text.contains("\"modelTimeMs\":2"));
}

#[test]
fn a_foreign_origin_is_refused_at_the_handshake() {
    let (gateway, addr, _) = start(true);
    assert!(connect_with(addr, Some("http://evil.example")).is_err());
    assert_eq!(gateway.clients(), 0);
}

#[test]
fn a_missing_origin_follows_the_policy() {
    let (_, strict, _) = start(false);
    assert!(connect_with(strict, None).is_err());
    let (_, lenient, _) = start(true);
    assert!(connect_with(lenient, None).is_ok());
}

#[test]
fn a_command_from_a_client_reaches_the_serial_line_as_a_frame() {
    let (gateway, addr, sink) = start(false);
    let mut client = connect_with(addr, Some(ORIGIN)).expect("connect");
    client
        .send(tungstenite::Message::text(call_json()))
        .expect("send");
    assert!(wait_until(|| !sink.0.lock().expect("sink").is_empty()));
    let bytes = sink.0.lock().expect("sink").clone();
    let mut payload = [0_u8; MAX_PAYLOAD];
    let (_, length) = decode_frame(&bytes[..bytes.len() - 1], &mut payload).expect("frame");
    let envelope = decode_message(&payload[..length]).expect("message");
    assert!(matches!(envelope.message, Message::CallFloor(_)));
    assert_eq!(gateway.refused(), 0);
}

#[test]
fn a_spoofed_telemetry_message_never_reaches_the_board() {
    let (gateway, addr, sink) = start(false);
    let mut client = connect_with(addr, Some(ORIGIN)).expect("connect");
    let spoof = to_json(&snapshot(5)).expect("json");
    client
        .send(tungstenite::Message::text(spoof))
        .expect("send");
    client
        .send(tungstenite::Message::text("not json"))
        .expect("send");
    assert!(wait_until(|| gateway.refused() == 2));
    assert!(sink.0.lock().expect("sink").is_empty());
}

#[test]
fn many_clients_all_get_the_same_line() {
    let (gateway, addr, _) = start(false);
    let mut clients: Vec<_> = (0..3)
        .map(|_| {
            let mut c = connect_with(addr, Some(ORIGIN)).expect("connect");
            set_timeout(&mut c);
            c
        })
        .collect();
    assert!(wait_until(|| gateway.clients() == 3));
    let lines = gateway.ingest(&board_frame(&snapshot(9)));
    for client in &mut clients {
        assert_eq!(
            next_text(client, Duration::from_secs(3)),
            Some(lines[0].clone())
        );
    }
}

#[test]
fn an_origin_header_that_is_not_readable_text_is_refused() {
    let (gateway, addr, _) = start(true);
    let mut request = format!("ws://{addr}")
        .into_client_request()
        .expect("request");
    request.headers_mut().insert(
        "Origin",
        tungstenite::http::HeaderValue::from_bytes(&[0xff, 0xfe]).expect("header"),
    );
    assert!(connect(request).is_err());
    assert_eq!(gateway.clients(), 0);
}
