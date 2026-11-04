use std::cell::OnceCell;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use lapsewright_core::convert::{self, ConvertError, Converted};
use lapsewright_core::metadata::VideoMetadata;
use lapsewright_core::output;
use lapsewright_core::runner::{Canceller, RunError};
use lapsewright_core::settings::Settings;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-ffmpeg");

/// The fake picks its behavior from the output file name, so `mode` names the output.
fn reserve_output(mode: &str) -> PathBuf {
    reserve_output_in(mode, mode)
}

/// Tests run in parallel, so two tests with the same `mode` need their own `dir`.
fn reserve_output_in(dir: &str, mode: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lapsewright-convert-{dir}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    output::reserve(&dir, OsStr::new(mode), "mp4").unwrap()
}

fn remove_dir(output: &Path) {
    fs::remove_dir_all(output.parent().unwrap()).unwrap();
}

/// Real files with a JPEG signature: the fake ignores them, `source::check` doesn't.
/// Written once, since tests run in parallel and a rewrite is briefly empty; the folder is reused.
fn images() -> Vec<PathBuf> {
    static IMAGES: OnceLock<Vec<PathBuf>> = OnceLock::new();
    IMAGES
        .get_or_init(|| {
            let dir = env::temp_dir().join("lapsewright-convert-photos");
            fs::create_dir_all(&dir).unwrap();
            let images: Vec<PathBuf> = (1..=3).map(|i| dir.join(format!("{i}.jpg"))).collect();
            images.iter().for_each(|p| fs::write(p, b"\xff\xd8\xff").unwrap());
            images
        })
        .clone()
}

fn run(images: &[PathBuf], output: &Path) -> (Result<Converted, ConvertError>, Vec<u64>) {
    let mut frames = Vec::new();
    let result = convert::run(Path::new(FAKE), &Settings::default(), images, &VideoMetadata::default(), output, drop, |p| {
        frames.push(p.frame.unwrap())
    });
    (result, frames)
}

#[test]
fn success_keeps_output_and_reports_progress() {
    let output = reserve_output("ok");
    let (result, frames) = run(&images(), &output);
    let done = result.expect("success");
    assert!(done.key_error.is_none());
    assert_eq!(done.log, ["Input #0, concat"]);
    assert_eq!(frames, [1, 2, 3]);
    assert!(output.is_file());
    remove_dir(&output);
}

#[test]
fn key_write_failure_keeps_output_and_reports_it() {
    // The fake writes no moov, so the keys have nowhere to go.
    let output = reserve_output_in("keys", "ok");
    let metadata = VideoMetadata { keys: vec![("com.apple.quicktime.make", "Sony".to_string())], ..Default::default() };
    let result = convert::run(Path::new(FAKE), &Settings::default(), &images(), &metadata, &output, drop, drop);
    assert!(result.expect("video done").key_error.is_some());
    assert!(output.is_file());
    remove_dir(&output);
}

#[test]
fn failure_deletes_output() {
    let output = reserve_output("fail");
    let (result, _) = run(&images(), &output);
    assert!(matches!(result, Err(ConvertError::Run(RunError::Failed { code: Some(1), .. }))));
    assert!(!output.exists());
    remove_dir(&output);
}

#[test]
fn fewer_frames_than_photos_fails_and_deletes_output() {
    let output = reserve_output("short");
    let (result, _) = run(&images(), &output);
    assert!(matches!(result, Err(ConvertError::Short { made: 2, expected: 3, .. })));
    assert!(!output.exists());
    remove_dir(&output);
}

#[test]
fn cancel_deletes_output() {
    let output = reserve_output("hang");
    let canceller = OnceCell::<Canceller>::new();
    let result = convert::run(
        Path::new(FAKE),
        &Settings::default(),
        &images(),
        &VideoMetadata::default(),
        &output,
        |c| canceller.set(c).ok().unwrap(),
        |_| canceller.get().unwrap().cancel().unwrap(),
    );
    assert!(matches!(result, Err(ConvertError::Run(RunError::Cancelled))));
    assert!(!output.exists());
    remove_dir(&output);
}

#[test]
fn no_images_deletes_output() {
    let output = reserve_output("empty");
    let (result, _) = run(&[], &output);
    assert!(matches!(result, Err(ConvertError::NoImages)));
    assert!(!output.exists());
    remove_dir(&output);
}
