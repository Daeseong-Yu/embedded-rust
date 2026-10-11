//! Reads telemetry frames from a serial port and prints them.
//!
//! Usage: cargo run -p decode -- /dev/cu.usbmodemXXXX

use std::{env, io, process::ExitCode, time::Duration};

use protocol::{Frame, MAX_FRAME_LEN, Message};

const BAUD_RATE: u32 = 115_200;

/// Running totals, printed when a frame is lost or rejected.
#[derive(Default)]
struct Stats {
    last_seq: Option<u16>,
    received: u64,
    lost: u64,
    rejected: u64,
}

impl Stats {
    /// Handles the bytes between two 0x00 delimiters.
    fn handle(&mut self, raw: &[u8]) {
        match protocol::decode(raw) {
            Ok(frame) => {
                self.track_seq(frame.seq);
                self.received += 1;
                print_frame(&frame);
            }
            Err(e) => {
                self.rejected += 1;
                eprintln!(
                    "rejected {} bytes: {e:?} (total {})",
                    raw.len(),
                    self.rejected
                );
            }
        }
    }

    /// Counts the frames missing between the previous seq and this one.
    fn track_seq(&mut self, seq: u16) {
        if let Some(last) = self.last_seq {
            let gap = seq.wrapping_sub(last).wrapping_sub(1);

            if gap != 0 {
                self.lost += u64::from(gap);
                eprintln!("lost {gap} frames before seq {seq} (total {})", self.lost);
            }
        }
        self.last_seq = Some(seq);
    }
}

fn print_frame(frame: &Frame) {
    let seq = frame.seq;

    match frame.msg {
        Message::Accel { x, y, z } => println!("{seq:5} accel x={x:6} y={y:6} z={z:6} mg"),
        Message::Mag { x, y, z } => println!("{seq:5} mag   x={x:6} y={y:6} z={z:6} mgauss"),
        Message::Gyro { x, y, z } => println!("{seq:5} gyro  x={x:6} y={y:6} z={z:6} mdps"),
    }
}

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: decode <serial port>");
        return ExitCode::FAILURE;
    };

    let mut port = match serialport::new(&path, BAUD_RATE)
        .timeout(Duration::from_secs(1))
        .open()
    {
        Ok(port) => port,
        Err(e) => {
            eprintln!("cannot open {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut stats = Stats::default();
    let mut frame: Vec<u8> = Vec::with_capacity(MAX_FRAME_LEN);
    let mut chunk = [0u8; 256];
    // The port usually opens in the middle of a frame, so drop bytes until
    // the first delimiter.
    let mut synced = false;

    loop {
        let n = match port.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => {
                eprintln!("read failed: {e}");
                return ExitCode::FAILURE;
            }
        };

        for &byte in &chunk[..n] {
            if byte != 0x00 {
                if synced && frame.len() <= MAX_FRAME_LEN {
                    frame.push(byte);
                }
                continue;
            }

            if synced && !frame.is_empty() {
                stats.handle(&frame);
            }

            synced = true;
            frame.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_seq_loses_nothing() {
        let mut stats = Stats::default();

        for seq in [10, 11, 12] {
            stats.track_seq(seq);
        }

        assert_eq!(stats.lost, 0);
    }

    #[test]
    fn gap_is_counted() {
        let mut stats = Stats::default();
        stats.track_seq(10);
        stats.track_seq(14);

        assert_eq!(stats.lost, 3);
    }

    #[test]
    fn wrap_around_is_not_a_gap() {
        let mut stats = Stats::default();
        stats.track_seq(u16::MAX);
        stats.track_seq(0);

        assert_eq!(stats.lost, 0);
    }

    #[test]
    fn bad_frame_is_rejected() {
        let mut stats = Stats::default();
        stats.handle(&[0x02, 0x01]);

        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.received, 0);
    }
}
