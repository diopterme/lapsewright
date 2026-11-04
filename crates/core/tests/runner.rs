use std::path::Path;

use lapsewright_core::runner::{self, RunError};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-ffmpeg");

fn start(mode: &str) -> runner::Job {
    runner::start(Path::new(FAKE), [mode]).expect("start fake ffmpeg")
}

#[test]
fn success_reports_every_block() {
    let job = start("ok");
    let frames: Vec<_> = job.progress.iter().map(|p| (p.frame, p.is_final)).collect();
    assert_eq!(frames, [(Some(1), false), (Some(2), false), (Some(3), true)]);
    assert_eq!(job.wait().expect("success"), ["Input #0, concat"]);
}

#[test]
fn failure_keeps_every_non_empty_stderr_line() {
    let job = start("fail");
    match job.wait() {
        Err(RunError::Failed { code, stderr }) => {
            assert_eq!(code, Some(1));
            let expected: Vec<String> = (1..=25).map(|i| format!("error line {i}")).collect();
            assert_eq!(stderr, expected);
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn cancel_kills_a_running_process() {
    let job = start("hang");
    assert_eq!(job.progress.recv().expect("first block").frame, Some(1));
    job.canceller().cancel().expect("kill");
    assert!(matches!(job.wait(), Err(RunError::Cancelled)));
}

#[test]
fn cancel_wins_over_exit_code() {
    let job = start("ok");
    job.progress.iter().for_each(drop);
    job.canceller().cancel().expect("kill");
    assert!(matches!(job.wait(), Err(RunError::Cancelled)));
}

#[test]
fn drains_both_pipes_without_deadlock() {
    let job = start("flood");
    let last = job.progress.iter().last().expect("progress");
    assert!(last.is_final);
    job.wait().expect("success");
}

#[test]
fn ignoring_progress_does_not_block() {
    start("flood").wait().expect("success");
}

#[test]
fn missing_binary_is_a_spawn_error() {
    let result = runner::start(Path::new("/no/such/ffmpeg"), ["ok"]);
    assert!(matches!(result, Err(RunError::Spawn { .. })));
}
