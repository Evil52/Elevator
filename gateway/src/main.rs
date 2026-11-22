use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use elevator_gateway::{Gateway, Journal, OriginPolicy};

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

fn die(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn main() {
    let options = parse_options();
    let port = serialport::new(&options.port, BAUD)
        .timeout(Duration::from_millis(100))
        .open()
        .unwrap_or_else(|error| die(&format!("cannot open {}: {error}", options.port)));
    let writer = port
        .try_clone()
        .unwrap_or_else(|error| die(&format!("cannot clone {}: {error}", options.port)));
    let mut reader = port;
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
        Box::new(writer),
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

    let incoming = Arc::clone(&gateway);
    let reading = thread::spawn(move || {
        let mut buffer = [0_u8; 256];
        let stdout = std::io::stdout();
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(error) => {
                    eprintln!("read failed: {error}");
                    return;
                }
            };
            let mut out = stdout.lock();
            for line in incoming.ingest(&buffer[..count]) {
                if writeln!(out, "{line}").is_err() {
                    return;
                }
            }
            if out.flush().is_err() {
                return;
            }
        }
    });

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if !line.trim().is_empty() && !gateway.command(&line) {
            eprintln!("command refused");
        }
    }
    let _ = reading.join();
}
