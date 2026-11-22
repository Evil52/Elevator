use std::io::{self, BufRead, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use elevator_gateway::link::supervise;
use elevator_gateway::{Gateway, Journal, OriginPolicy};
use serialport::SerialPort;

const DEFAULT_PORT: &str = "/dev/ttyACM0";
const DEFAULT_LISTEN: &str = "127.0.0.1:8787";
const DEFAULT_ORIGIN: &str = "http://localhost:5173";
const BAUD: u32 = 115_200;
const DEFAULT_JOURNAL_SIZE: usize = 10_000;

struct Options {
    port: String,
    listen: String,
    origins: Vec<String>,
    allow_no_origin: bool,
    journal_file: Option<String>,
    journal_size: usize,
}

fn parse_options() -> Options {
    let mut options = Options {
        port: DEFAULT_PORT.to_owned(),
        listen: DEFAULT_LISTEN.to_owned(),
        origins: Vec::new(),
        allow_no_origin: false,
        journal_file: None,
        journal_size: DEFAULT_JOURNAL_SIZE,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => options.listen = args.next().unwrap_or(options.listen),
            "--origin" => {
                if let Some(origin) = args.next() {
                    options.origins.push(origin);
                }
            }
            "--allow-no-origin" => options.allow_no_origin = true,
            "--journal" => options.journal_file = args.next(),
            "--journal-size" => {
                if let Some(size) = args.next().and_then(|text| text.parse().ok()) {
                    options.journal_size = size;
                }
            }
            _ => options.port = arg,
        }
    }
    if options.origins.is_empty() {
        options.origins.push(DEFAULT_ORIGIN.to_owned());
    }
    options
}

type Port = Box<dyn SerialPort>;

fn open_port(path: &str) -> io::Result<(Port, Port)> {
    let port = serialport::new(path, BAUD)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(io::Error::other)?;
    let writer = port.try_clone().map_err(io::Error::other)?;
    Ok((port, writer))
}

fn die(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn main() {
    let options = parse_options();
    let mut journal = Journal::new(options.journal_size);
    if let Some(path) = &options.journal_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|error| die(&format!("cannot open journal {path}: {error}")));
        journal = journal.with_sink(Box::new(file));
    }
    let gateway = Gateway::with_journal(
        Box::new(std::io::sink()),
        OriginPolicy::new(options.origins.clone(), options.allow_no_origin),
        journal,
    );

    let listener = TcpListener::bind(&options.listen)
        .unwrap_or_else(|error| die(&format!("cannot listen on {}: {error}", options.listen)));
    eprintln!(
        "listening on {} for origins {:?}",
        options.listen, options.origins
    );
    let server = Arc::clone(&gateway);
    thread::spawn(move || server.serve(&listener));

    gateway.set_writer(None);
    let board = Arc::clone(&gateway);
    let path = options.port.clone();
    let supervisor = thread::spawn(move || {
        let mut connected = false;
        let mut last_error = String::new();
        let stdout = std::io::stdout();
        supervise(
            &board,
            || {
                if connected {
                    eprintln!("board link lost");
                    connected = false;
                }
                match open_port(&path) {
                    Ok(pair) => {
                        eprintln!("board link up on {path}");
                        connected = true;
                        last_error.clear();
                        Ok(pair)
                    }
                    Err(error) => {
                        let text = error.to_string();
                        if text != last_error {
                            eprintln!("board link down: {text}");
                            last_error = text;
                        }
                        Err(error)
                    }
                }
            },
            thread::sleep,
            || false,
            |lines| {
                let mut out = stdout.lock();
                for line in lines {
                    if writeln!(out, "{line}").is_err() {
                        return;
                    }
                }
                let _ = out.flush();
            },
        );
    });

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if !line.trim().is_empty() && !gateway.command(&line) {
            eprintln!("command refused");
        }
    }
    let _ = supervisor.join();
}
