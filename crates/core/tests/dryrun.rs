//! Encodes every profile × allowed chroma × allowed range with real ffmpeg.
//! Needs ffmpeg/ffprobe with zimg (override with LAPSEWRIGHT_FFMPEG / LAPSEWRIGHT_FFPROBE):
//! `cargo test -p lapsewright-core --test dryrun -- --ignored`

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use lapsewright_core::builder;
use lapsewright_core::convert::{self, ConvertError};
use lapsewright_core::metadata::VideoMetadata;
use lapsewright_core::output;
use lapsewright_core::runner::{Canceller, RunError};
use lapsewright_core::settings::{Codec, DynamicRange, MotionBlur, Profile, Resolution, Settings};

const FRAMES: usize = 24;
const RANGES: [DynamicRange; 3] = [DynamicRange::Sdr, DynamicRange::Hlg, DynamicRange::Hdr];

fn tool(var: &str, default: &str) -> PathBuf {
    env::var_os(var).map(PathBuf::from).unwrap_or_else(|| default.into())
}

/// 4:3 frames so the 16:9 output exercises the aspect crop.
/// Numbered from 1, the way `builder::build` reads photos.
fn make_sequence(ffmpeg: &Path, dir: &Path, ext: &str) -> PathBuf {
    let pattern = dir.join(format!("img_%02d.{ext}"));
    let status = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=400x300:r=30"])
        .args(["-frames:v", &FRAMES.to_string()])
        .arg(&pattern)
        .status()
        .expect("run ffmpeg");
    assert!(status.success(), "could not generate {ext} frames");

    pattern
}

fn base() -> Settings {
    Settings { resolution: Resolution::new(320, 180).unwrap(), ..Settings::default() }
}

fn jpg_cases() -> Vec<Settings> {
    let combos = Profile::ALL.into_iter().flat_map(|profile| {
        let spec = profile.spec();
        let ranges: &[DynamicRange] = if spec.hdr { &RANGES } else { &RANGES[..1] };
        spec.chroma
            .iter()
            .flat_map(move |&chroma| ranges.iter().map(move |&range| Settings { profile, chroma, range, ..base() }))
    });
    let blurs = [MotionBlur::Off, MotionBlur::Frames10, MotionBlur::Frames15, MotionBlur::Frames20]
        .map(|blur| Settings { blur, ..base() });
    combos.chain(blurs).collect()
}

fn png_cases() -> Vec<Settings> {
    [
        (Profile::H264High, DynamicRange::Sdr),
        (Profile::HevcMain10, DynamicRange::Hlg),
        (Profile::ProResHq, DynamicRange::Hdr),
    ]
    .map(|(profile, range)| Settings { profile, range, ..base() })
    .to_vec()
}

fn probe(ffprobe: &Path, file: &Path) -> String {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=codec_tag_string,width,height,nb_frames:stream_side_data=side_data_type"])
        .args(["-of", "default=nw=1"])
        .arg(file)
        .output()
        .expect("run ffprobe");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Returns a problem description, or None when the output looks right.
fn encode(ffmpeg: &Path, ffprobe: &Path, settings: &Settings, photos: &Path, output: &Path) -> Option<String> {
    let out = Command::new(ffmpeg).args(builder::build(settings, photos, FRAMES, &VideoMetadata::default(), output)).output().expect("run ffmpeg");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        return Some(format!("exit {:?}: {}", out.status.code(), tail.join(" | ")));
    }
    let info = probe(ffprobe, output);
    let has = |line: &str| info.lines().any(|l| l == line);
    let frames: u32 = info.lines().find_map(|l| l.strip_prefix("nb_frames=")?.parse().ok()).unwrap_or(0);
    let problems = [
        (!has("width=320") || !has("height=180"), "size is not 320x180"),
        (frames == 0, "no frames"),
        (settings.profile.codec() == Codec::Hevc && !has("codec_tag_string=hvc1"), "HEVC tag is not hvc1"),
        (
            settings.range == DynamicRange::Hdr
                && !(has("side_data_type=Mastering display metadata") && has("side_data_type=Content light level metadata")),
            "HDR static metadata missing",
        ),
    ];
    let failed: Vec<&str> = problems.iter().filter(|(bad, _)| *bad).map(|(_, why)| *why).collect();
    (!failed.is_empty()).then(|| format!("{} ({})", failed.join(", "), info.replace('\n', " ")))
}

#[test]
#[ignore = "needs ffmpeg with zimg; run with --ignored"]
fn every_combination_encodes() {
    let ffmpeg = tool("LAPSEWRIGHT_FFMPEG", "ffmpeg");
    let ffprobe = tool("LAPSEWRIGHT_FFPROBE", "ffprobe");
    let dir = env::temp_dir().join(format!("lapsewright-dryrun-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    let runs = [("jpg", jpg_cases()), ("png", png_cases())];
    let mut total = 0;
    let mut failures = Vec::new();
    for (ext, cases) in runs {
        let photos = make_sequence(&ffmpeg, &dir, ext);
        for (i, settings) in cases.iter().enumerate() {
            let label = format!(
                "{ext} {:?} {:?} {:?} blur{}",
                settings.profile, settings.chroma, settings.range, settings.blur.frames()
            );
            let output = dir.join(format!("{ext}_{i:02}.{}", settings.profile.codec().extension()));
            total += 1;
            if let Some(problem) = encode(&ffmpeg, &ffprobe, settings, &photos, &output) {
                failures.push(format!("{label}: {problem}"));
            }
        }
    }

    assert!(failures.is_empty(), "{} of {total} failed (files in {}):\n{}", failures.len(), dir.display(), failures.join("\n"));
    fs::remove_dir_all(&dir).unwrap();
    println!("{total} encodes ok");
}

#[test]
#[ignore = "needs ffmpeg with libjxl; run with --ignored"]
fn truncated_jxl_fails_instead_of_cutting_the_video_short() {
    let ffmpeg = tool("LAPSEWRIGHT_FFMPEG", "ffmpeg");
    let dir = env::temp_dir().join(format!("lapsewright-dryrun-broken-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let whole = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gps.jxl")).unwrap();
    let images: Vec<PathBuf> = ["1.jxl", "2.jxl", "3.jxl"].iter().map(|name| dir.join(name)).collect();
    for (i, image) in images.iter().enumerate() {
        fs::write(image, if i == 1 { &whole[..whole.len() / 2] } else { &whole[..] }).unwrap();
    }
    let output = output::reserve(&dir, OsStr::new("broken"), "mp4").unwrap();

    let canceller = Arc::new(Mutex::new(None::<Canceller>));
    let (done, result) = mpsc::channel();
    let shared = Arc::clone(&canceller);
    let settings = Settings { blur: MotionBlur::Off, ..base() };
    thread::spawn(move || {
        let on_start = |c| *shared.lock().unwrap() = Some(c);
        let _ = done.send(convert::run(&ffmpeg, &settings, &images, &VideoMetadata::default(), &output, on_start, drop));
    });
    let result = result.recv_timeout(Duration::from_secs(20)).unwrap_or_else(|_| {
        if let Some(c) = canceller.lock().unwrap().as_ref() {
            let _ = c.cancel();
        }
        panic!("ffmpeg still running after 20 s");
    });
    assert!(matches!(result, Err(ConvertError::Run(RunError::Failed { .. }))), "{result:?}");
    fs::remove_dir_all(&dir).unwrap();
}
