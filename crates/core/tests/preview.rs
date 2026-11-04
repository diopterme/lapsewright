//! Makes previews with real ffmpeg (override with LAPSEWRIGHT_FFMPEG):
//! `cargo test -p lapsewright-core --test preview -- --ignored`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lapsewright_core::preview::{self, PreviewError};
use lapsewright_core::runner::RunError;
use lapsewright_core::settings::{MotionBlur, Settings};

fn ffmpeg() -> PathBuf {
    env::var_os("LAPSEWRIGHT_FFMPEG").map(PathBuf::from).unwrap_or_else(|| "ffmpeg".into())
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

fn jpeg_size(bytes: &[u8]) -> (u16, u16) {
    // SOF0 marker: FF C0, length (2), precision (1), height (2), width (2).
    let at = bytes.windows(2).position(|w| w == [0xFF, 0xC0]).expect("SOF0");
    let be = |i: usize| u16::from_be_bytes([bytes[at + i], bytes[at + i + 1]]);
    (be(7), be(5))
}

#[test]
#[ignore]
fn blurred_preview_is_one_jpeg_within_the_side() {
    let images = vec![fixture("gps.jxl"); 3];
    let settings = Settings { blur: MotionBlur::Frames5, ..Settings::default() };
    let jpeg = preview::start(&ffmpeg(), &images, 1, &settings).unwrap().wait().unwrap();
    assert_eq!(jpeg[..2], [0xFF, 0xD8]);
    let (w, h) = jpeg_size(&jpeg);
    assert!(w.max(h) as u32 <= preview::SIDE, "{w}x{h}");
}

#[test]
#[ignore]
fn truncated_jxl_fails_instead_of_hanging() {
    let dir = env::temp_dir().join(format!("lapsewright-preview-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("broken.jxl");
    let whole = fs::read(fixture("gps.jxl")).unwrap();
    fs::write(&broken, &whole[..whole.len() / 2]).unwrap();

    let started = Instant::now();
    let result = preview::start(&ffmpeg(), &[broken], 0, &Settings::default()).unwrap().wait();
    assert!(matches!(result, Err(PreviewError::Run(RunError::Failed { .. }))), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    fs::remove_dir_all(&dir).unwrap();
}
