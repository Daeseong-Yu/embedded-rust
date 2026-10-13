//! Reads telemetry frames from the board over a serial port.
//!
//! Usage: cargo run -p gateway -- /dev/cu.usbmodemXXXX

mod deframer;

use deframer::Deframer;
use std::{env, io, process::ExitCode, time::Duration};

const BAUD_RATE: u32 = 115_200;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: gateway <serial port>");
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

    let mut deframer = Deframer::new();
    let mut chunk = [0u8; 256];

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
            match deframer.push(byte) {
                Some(Ok(frame)) => println!("{frame:?}"),
                Some(Err(e)) => eprintln!("rejected: {e:?}"),
                None => {}
            }
        }
    }
}
