use std::io::{self, ErrorKind, Read, Write};
use std::time::Duration;

use crate::Gateway;

pub const BACKOFF_BASE: Duration = Duration::from_millis(500);
pub const BACKOFF_MAX: Duration = Duration::from_secs(5);

fn backoff(attempt: u32) -> Duration {
    let factor = 1_u32.checked_shl(attempt).unwrap_or(u32::MAX);
    BACKOFF_BASE.saturating_mul(factor).min(BACKOFF_MAX)
}

fn read_until_lost<R: Read>(
    gateway: &Gateway,
    reader: &mut R,
    stop: &impl Fn() -> bool,
    on_lines: &mut impl FnMut(&[String]),
) {
    let mut buffer = [0_u8; 256];
    while !stop() {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                let lines = gateway.ingest(&buffer[..count]);
                if !lines.is_empty() {
                    on_lines(&lines);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::TimedOut | ErrorKind::Interrupted | ErrorKind::WouldBlock
                ) => {}
            Err(_) => return,
        }
    }
}

pub const PING_EVERY: Duration = Duration::from_secs(1);

pub fn heartbeat(
    gateway: &Gateway,
    period: Duration,
    mut sleep: impl FnMut(Duration),
    stop: impl Fn() -> bool,
) {
    while !stop() {
        gateway.ping_board();
        sleep(period);
    }
}

pub fn supervise<R, W>(
    gateway: &Gateway,
    mut open: impl FnMut() -> io::Result<(R, W)>,
    mut sleep: impl FnMut(Duration),
    stop: impl Fn() -> bool,
    mut on_lines: impl FnMut(&[String]),
) where
    R: Read,
    W: Write + Send + 'static,
{
    let mut attempt = 0_u32;
    while !stop() {
        match open() {
            Err(_) => {
                sleep(backoff(attempt));
                attempt = attempt.saturating_add(1);
            }
            Ok((mut reader, writer)) => {
                attempt = 0;
                gateway.reset_stream();
                gateway.set_writer(Some(Box::new(writer)));
                gateway.board_link(true);
                read_until_lost(gateway, &mut reader, &stop, &mut on_lines);
                gateway.set_writer(None);
                gateway.board_link(false);
                sleep(BACKOFF_BASE);
            }
        }
    }
}
