use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use elevator_gateway::{Bridge, to_json};

const DEFAULT_PORT: &str = "/dev/ttyACM0";
const BAUD: u32 = 115_200;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PORT.to_owned());
    let port = match serialport::new(&path, BAUD)
        .timeout(Duration::from_millis(100))
        .open()
    {
        Ok(port) => port,
        Err(error) => {
            eprintln!("cannot open {path}: {error}");
            std::process::exit(1);
        }
    };
    let mut writer = match port.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            eprintln!("cannot clone {path}: {error}");
            std::process::exit(1);
        }
    };
    let mut reader = port;
    let bridge = Arc::new(Mutex::new(Bridge::new()));

    let incoming = Arc::clone(&bridge);
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
            let envelopes = match incoming.lock() {
                Ok(mut bridge) => bridge.feed(&buffer[..count]),
                Err(_) => return,
            };
            let mut out = stdout.lock();
            for envelope in envelopes {
                match to_json(&envelope) {
                    Ok(line) => {
                        if writeln!(out, "{line}").is_err() {
                            return;
                        }
                    }
                    Err(error) => eprintln!("json failed: {error}"),
                }
            }
            if out.flush().is_err() {
                return;
            }
        }
    });

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let frame = match bridge.lock() {
            Ok(mut bridge) => bridge.command_frame(&line),
            Err(_) => break,
        };
        match frame {
            Ok(bytes) => {
                if let Err(error) = writer.write_all(&bytes) {
                    eprintln!("write failed: {error}");
                    break;
                }
            }
            Err(error) => eprintln!("command refused: {error:?}"),
        }
    }
    let _ = reading.join();
}
