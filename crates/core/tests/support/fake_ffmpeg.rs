//! Stand-in for ffmpeg in runner and convert tests. A lone argument picks the behavior;
//! with a full ffmpeg argument list, the output file name (last argument) does.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

const FLOOD_LINES: usize = 50_000;

fn block(out: &mut impl Write, frame: usize, end: bool) {
    let state = if end { "end" } else { "continue" };
    writeln!(out, "frame={frame}\nfps=30.00\nout_time_us={}\nspeed=1.0x\nprogress={state}", frame * 33_333).unwrap();
    out.flush().unwrap();
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.as_slice() {
        [mode] => mode.clone(),
        [.., output] => Path::new(output).file_stem().unwrap_or_default().to_string_lossy().into_owned(),
        [] => String::new(),
    };
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    match mode.as_str() {
        "ok" => {
            writeln!(err, "Input #0, concat").unwrap();
            (1..=3).for_each(|f| block(&mut out, f, f == 3));
            ExitCode::SUCCESS
        }
        // Exits 0 with fewer frames than photos, as ffmpeg does at an empty photo.
        "short" => {
            (1..=2).for_each(|f| block(&mut out, f, f == 2));
            ExitCode::SUCCESS
        }
        "fail" => {
            block(&mut out, 1, false);
            for i in 1..=25 {
                writeln!(err, "error line {i}\n").unwrap();
            }
            ExitCode::from(1)
        }
        "hang" => {
            block(&mut out, 1, false);
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        // Fills both pipe buffers many times over; blocks forever unless both are drained.
        "flood" => {
            for i in 1..=FLOOD_LINES {
                writeln!(err, "stderr noise {i:08} ................................................").unwrap();
                block(&mut out, i, i == FLOOD_LINES);
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("unknown mode: {mode}");
            ExitCode::from(2)
        }
    }
}
