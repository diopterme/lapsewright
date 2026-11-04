//! Photos to video through the real binary and real ffmpeg.
//! Needs ffmpeg/ffprobe with zimg on PATH (override with LAPSEWRIGHT_FFMPEG / LAPSEWRIGHT_FFPROBE):
//! `cargo test -p lapsewright-cli --test e2e -- --ignored`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PHOTOS: usize = 12;

/// `binaries::locate` wants a real file, so a bare name is looked up on PATH.
fn tool(var: &str, name: &str) -> PathBuf {
    env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
        let file = format!("{name}{}", env::consts::EXE_SUFFIX);
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(&file))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("{name} not on PATH"))
    })
}

#[test]
#[ignore = "needs ffmpeg with zimg; run with --ignored"]
fn converts_a_folder_of_photos() {
    let ffmpeg = tool("LAPSEWRIGHT_FFMPEG", "ffmpeg");
    let ffprobe = tool("LAPSEWRIGHT_FFPROBE", "ffprobe");
    let root = env::temp_dir().join(format!("lapsewright-e2e-{}", std::process::id()));
    // A quote and a space in the path exercise the photo links.
    let photos = root.join("day 1's trip");
    let out = root.join("out");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&photos).unwrap();
    fs::create_dir_all(&out).unwrap();

    // 4:3 photos, unpadded numbers (img_1 … img_12).
    let status = Command::new(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=400x300:r=30"])
        .args(["-frames:v", &PHOTOS.to_string()])
        .arg(photos.join("img_%d.jpg"))
        .status()
        .unwrap();
    assert!(status.success(), "could not make photos");

    let run = Command::new(env!("CARGO_BIN_EXE_lapsewright"))
        .arg("--ffmpeg")
        .arg(&ffmpeg)
        .arg("convert")
        .arg(&photos)
        .arg("--out")
        .arg(&out)
        .args(["--codec", "hevc", "--range", "hlg", "--size", "320x180"])
        .output()
        .unwrap();
    assert!(run.status.success(), "convert failed: {}", String::from_utf8_lossy(&run.stderr));

    let video = out.join("day 1's trip.mov");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), video.to_str().unwrap());
    let info = probe(&ffprobe, &video);
    for expected in [
        "codec_name=hevc",
        "codec_tag_string=hvc1",
        "profile=Main 10",
        "pix_fmt=yuv420p10le",
        "width=320",
        "height=180",
        "color_range=tv",
        "color_space=bt2020nc",
        "color_transfer=arib-std-b67",
        "color_primaries=bt2020",
        &format!("nb_frames={PHOTOS}"),
    ] {
        assert!(info.lines().any(|l| l == expected), "missing {expected} in:\n{info}");
    }
    fs::remove_dir_all(&root).unwrap();
}

/// The fixture says 14:22:10 at +09:00: `mvhd` holds 05:22:10 UTC, the Apple key the local time.
#[test]
#[ignore = "needs ffmpeg with zimg; run with --ignored"]
fn carries_photo_metadata_into_the_video() {
    let ffmpeg = tool("LAPSEWRIGHT_FFMPEG", "ffmpeg");
    let ffprobe = tool("LAPSEWRIGHT_FFPROBE", "ffprobe");
    let root = env::temp_dir().join(format!("lapsewright-e2e-meta-{}", std::process::id()));
    let photos = root.join("seoul");
    let out = root.join("out");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&photos).unwrap();
    fs::create_dir_all(&out).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gps.jpg");
    for i in 1..=3 {
        fs::copy(&fixture, photos.join(format!("img_{i}.jpg"))).unwrap();
    }
    let convert = |args: &[&str]| {
        let run = Command::new(env!("CARGO_BIN_EXE_lapsewright"))
            .arg("--ffmpeg")
            .arg(&ffmpeg)
            .arg("convert")
            .arg(&photos)
            .arg("--out")
            .arg(&out)
            .args(["--size", "320x180"])
            .args(args)
            .output()
            .unwrap();
        assert!(run.status.success(), "convert failed: {}", String::from_utf8_lossy(&run.stderr));
        PathBuf::from(String::from_utf8_lossy(&run.stdout).trim())
    };

    let mov = format_tags(&ffprobe, &convert(&["--codec", "hevc"]));
    let mp4 = format_tags(&ffprobe, &convert(&["--codec", "h264", "--location", "-33.8568,151.2153"]));

    for expected in [
        "creation_time=2026-06-01T05:22:10.000000Z",
        "location=+37.566500+126.978000/",
        "make=SONY",
        "model=ILCE-7SM3",
        // `author` is stored as `©aut`, which ffprobe reports as `artist`.
        "artist=Kim",
        "copyright=Copyright 2026 Kim",
        "com.apple.quicktime.creationdate=2026-06-01T14:22:10+0900",
        "com.apple.quicktime.camera.lens_model=FE 24mm F1.4 GM",
    ] {
        assert!(mov.lines().any(|l| l == expected), "mov is missing {expected} in:\n{mov}");
    }
    // MP4 keeps location in `loci`, which stores 4 decimals.
    for expected in [
        "creation_time=2026-06-01T05:22:10.000000Z",
        "location=-33.8568+151.2153/",
        "copyright=Copyright 2026 Kim",
        "com.apple.quicktime.creationdate=2026-06-01T14:22:10+0900",
        "com.apple.quicktime.make=SONY",
        "com.apple.quicktime.model=ILCE-7SM3",
    ] {
        assert!(mp4.lines().any(|l| l == expected), "mp4 is missing {expected} in:\n{mp4}");
    }
    fs::remove_dir_all(&root).unwrap();
}

/// ffmpeg picks one decoder from the first photo, so a second format would fail mid-way.
#[test]
fn mixed_formats_are_refused() {
    let photos = env::temp_dir().join(format!("lapsewright-e2e-mixed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&photos);
    fs::create_dir_all(&photos).unwrap();
    for name in ["1.jpg", "2.JPEG", "3.jxl"] {
        fs::write(photos.join(name), b"").unwrap();
    }
    let run = Command::new(env!("CARGO_BIN_EXE_lapsewright")).arg("args").arg(&photos).output().unwrap();
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("mixed image formats (JPG, JXL)"), "{stderr}");
    fs::remove_dir_all(&photos).unwrap();
}

fn format_tags(ffprobe: &Path, file: &Path) -> String {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries", "format_tags", "-of", "default=nw=1:nk=0"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).replace("TAG:", "")
}

fn probe(ffprobe: &Path, file: &Path) -> String {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=codec_name,codec_tag_string,profile,pix_fmt,width,height,color_range,color_space,color_transfer,color_primaries,nb_frames"])
        .args(["-of", "default=nw=1"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}
