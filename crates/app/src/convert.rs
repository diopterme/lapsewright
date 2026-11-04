use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use jiff::Zoned;
use lapsewright_core::convert::{self, ConvertError, LinkError};
use lapsewright_core::exif;
use lapsewright_core::runner::{Canceller, RunError};
use lapsewright_core::settings::Settings;
use lapsewright_core::store::SettingsFile;
use lapsewright_core::{builder, metadata, output, rules};
use serde::{Deserialize, Serialize};

use crate::folder;
use crate::lang::{is_ko, t};
use crate::prefs::Prefs;
use crate::session;

fn disk_full() -> &'static str {
    t("디스크 공간이 모자람 → 출력 폴더를 다른 디스크로", "The disk is full → pick an output folder on another disk")
}

fn no_write() -> &'static str {
    t("출력 폴더에 쓸 수 없음", "Can’t write to the output folder")
}

fn no_ffmpeg() -> &'static str {
    t("ffmpeg 를 찾을 수 없음 → 앱을 다시 설치", "ffmpeg is missing → reinstall the app")
}

fn unreadable(photo: &Path) -> String {
    if is_ko() { format!("사진을 못 읽음: {}", name(photo)) } else { format!("Can’t read photo: {}", name(photo)) }
}

/// What [시작] took from the window (V2): changing the window afterwards doesn't reach it.
#[derive(Deserialize)]
pub struct Job {
    pub folder: PathBuf,
    pub settings: SettingsFile,
    pub output: PathBuf,
    /// Core `Settings::crop`: per folder, so not in `settings`.
    pub crop: u16,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// `file`: the name is picked now, when this job starts (B2).
    Started { file: PathBuf, command: String, frames: usize },
    Progress { frame: u64, fps: Option<f64>, bitrate_kbps: Option<f64> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub file: PathBuf,
    pub size: u64,
    /// The video plays, but the Apple date and lens keys are missing (yellow band).
    pub key_error: Option<String>,
    pub log: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub cancelled: bool,
    /// E3: plain words for common causes.
    pub message: String,
    pub log: Vec<String>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Self {
        Failure { cancelled: false, message: message.into(), log: Vec::new() }
    }
}

/// One conversion, like the CLI's `convert`. The video gets the first photo's EXIF, with the
/// settings window's author and copyright over it; an edited location comes with ⑤.
pub fn run(ffmpeg: &Path, job: &Job, prefs: &Prefs, on_start: impl FnOnce(Canceller), mut on_event: impl FnMut(Event)) -> Result<Done, Failure> {
    let (folder, images) = folder::open(&job.folder).map_err(Failure::new)?;
    let settings = rules::normalize(Settings { crop: job.crop, ..job.settings.to_settings().map_err(Failure::new)? });
    // Unreadable EXIF costs only the date and camera tags, not the video.
    let exif = exif::read(&images[0]).unwrap_or_default();
    let now = Zoned::now();
    let metadata = metadata::build(&exif, exif.gps, Some(&prefs.author), Some(&prefs.copyright), now.time_zone(), now.year());
    let file = output::reserve(&job.output, OsStr::new(&folder.name), settings.profile.codec().extension())
        .map_err(|e| Failure::new(io_reason(&e)))?;
    // The photo links are temporary, so the log shows a placeholder, as `lapsewright args` does.
    let command = builder::command_line(&builder::build(&settings, Path::new("<photos>"), images.len(), &metadata, &file));
    on_event(Event::Started { file: file.clone(), command, frames: images.len() });

    let mut last_frame = 0;
    let result = convert::run(ffmpeg, &settings, &images, &metadata, &file, on_start, |p| {
        last_frame = p.frame.unwrap_or(last_frame);
        on_event(Event::Progress { frame: last_frame, fps: p.fps, bitrate_kbps: p.bitrate_kbps });
    });
    match result {
        Ok(done) => Ok(Done {
            size: fs::metadata(&file).map_or(0, |m| m.len()),
            file,
            key_error: done.key_error.map(|e| e.to_string()),
            log: done.log,
        }),
        Err(ConvertError::Run(RunError::Cancelled)) => Err(Failure { cancelled: true, message: t("취소됨", "Cancelled").to_string(), log: Vec::new() }),
        Err(ConvertError::Run(RunError::Failed { code, stderr })) => {
            // Frame n blends photos n to n + blur − 1, so the last one it needed is the suspect.
            // Before the first frame, any of the first `blur` photos could be it (measured on 30 JXL).
            let blended = settings.blur.frames().saturating_sub(1) as u64;
            let near = if last_frame == 0 && blended > 0 {
                None
            } else {
                usize::try_from(last_frame + blended).ok().and_then(|i| images.get(i)).or(images.last())
            };
            Err(Failure { cancelled: false, message: ffmpeg_reason(&stderr, code, near.map(PathBuf::as_path), &images), log: stderr })
        }
        Err(ConvertError::Link(LinkError::Photo { photo, .. })) => Err(Failure::new(unreadable(&photo))),
        Err(ConvertError::Short { made, log, .. }) => {
            // Measured: the video ends right before the empty or missing photo.
            let photo = usize::try_from(made).ok().and_then(|i| images.get(i)).or(images.last());
            Err(Failure { cancelled: false, message: photo.map_or(t("사진을 못 읽음", "Can’t read a photo").to_string(), |p| unreadable(p)), log })
        }
        Err(ConvertError::Run(RunError::Spawn { source, .. })) if source.kind() == io::ErrorKind::NotFound => Err(Failure::new(no_ffmpeg())),
        Err(e) => Err(Failure::new(e.to_string())),
    }
}

/// E3: ffmpeg only speaks English; known sentences become plain words, anything else its exit code.
/// `near`: the photo the last frame needed, since decoders don't name the file they choke on.
fn ffmpeg_reason(stderr: &[String], code: Option<i32>, near: Option<&Path>, images: &[PathBuf]) -> String {
    let has = |needle: &str| stderr.iter().any(|line| line.contains(needle));
    if has("No space left on device") {
        return disk_full().to_string();
    }
    // Before "Permission denied": an unreadable photo is reported with both.
    if let Some(photo) = stderr.iter().find_map(|line| unopened(line, images)) {
        return unreadable(photo);
    }
    if has("Permission denied") {
        return no_write().to_string();
    }
    if has("Invalid data found when processing input") || has("Decoding error") {
        return match near {
            Some(photo) if is_ko() => format!("깨진 사진이 있음: {} 근처", name(photo)),
            Some(photo) => format!("A broken photo near {}", name(photo)),
            None => t("깨진 사진이 있음", "There is a broken photo").to_string(),
        };
    }
    let code = code.map_or(t("없음", "none").to_string(), |c| c.to_string());
    if is_ko() { format!("ffmpeg 오류 (코드 {code})") } else { format!("ffmpeg error (code {code})") }
}

/// `Could not open file : /tmp/lapsewright-1-0/000004.jxl` names a link; its number is the photo's place.
fn unopened<'a>(line: &str, images: &'a [PathBuf]) -> Option<&'a Path> {
    let link = Path::new(line.split_once("Could not open file : ")?.1);
    let number: usize = link.file_stem()?.to_str()?.parse().ok()?;
    images.get(number.checked_sub(1)?).map(PathBuf::as_path)
}

fn io_reason(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::StorageFull => disk_full().to_string(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => no_write().to_string(),
        io::ErrorKind::NotFound => t("출력 폴더가 없음", "The output folder is missing").to_string(),
        _ if is_ko() => format!("영상 파일을 못 만듦: {e}"),
        _ => format!("Couldn’t create the video file: {e}"),
    }
}

fn name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// B1–B3: what the start line shows for `dir`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub problem: Option<String>,
    /// The file name [시작] would get now, `_2` included.
    pub file: Option<String>,
    pub free: Option<u64>,
}

pub fn target(dir: &Path, name: &str, extension: &str) -> Target {
    let problem = session::output_problem(dir);
    let file = problem.is_none().then(|| self::name(&output::next_free(dir, OsStr::new(name), extension)));
    Target { problem, file, free: session::free_space(dir) }
}

/// V4: the Mac stays awake while this lives; the screen may still sleep.
pub struct Awake(Option<Child>);

impl Awake {
    pub fn new() -> Self {
        // `-w`: caffeinate also ends if the app dies without dropping this.
        #[cfg(target_os = "macos")]
        let child = Command::new("/usr/bin/caffeinate").args(["-i", "-w", &std::process::id().to_string()]).spawn();
        #[cfg(not(target_os = "macos"))]
        let child: io::Result<Child> = Err(io::ErrorKind::Unsupported.into());
        // Only sleep is no longer blocked; the conversion itself is fine.
        Awake(child.inspect_err(|e| eprintln!("could not keep the Mac awake: {e}")).ok())
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Finder with the video selected (`reveal`), or the video in its default player.
pub fn show(path: &Path, reveal: bool) -> Result<(), String> {
    if !path.is_file() {
        return Err(t("영상이 없음 (옮겼거나 지움)", "The video is gone (moved or deleted)").to_string());
    }
    if !reveal && !path.extension().is_some_and(|e| e == "mp4" || e == "mov") {
        return Err(t("영상 파일이 아님", "Not a video file").to_string());
    }
    if !cfg!(target_os = "macos") {
        return Err(t("아직 맥에서만 됨", "Only on the Mac for now").to_string());
    }
    let mut open = Command::new("/usr/bin/open");
    if reveal {
        open.arg("-R");
    }
    let status = open.arg(path).status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else if is_ko() {
        Err(format!("열지 못함 (코드 {status})"))
    } else {
        Err(format!("Couldn’t open it (code {status})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn common_ffmpeg_failures_get_plain_words() {
        let full = lines(&["[out#0/mp4 @ 0x1] Error writing trailer: No space left on device"]);
        assert_eq!(ffmpeg_reason(&full, Some(1), None, &[]), disk_full());
        let write = lines(&["[out#0/mp4 @ 0x1] Error opening output /v/a.mp4: Permission denied"]);
        assert_eq!(ffmpeg_reason(&write, Some(1), None, &[]), no_write());
        // Measured with ffmpeg 9.0.2: the 4th photo unreadable (chmod 000) or deleted mid-way.
        let photos: Vec<PathBuf> = (1..=6).map(|i| PathBuf::from(format!("/p/it is/p{i}.jpg"))).collect();
        let gone = lines(&["[in#0/image2 @ 0x1] Could not open file : /tmp/lapsewright-1-0/000004.jpg", "Permission denied"]);
        assert_eq!(ffmpeg_reason(&gone, Some(243), None, &photos), "사진을 못 읽음: p4.jpg");
        // Measured: random bytes named .jpg.
        let broken = lines(&["[vist#0:0/mjpeg @ 0x1] [dec:mjpeg @ 0x2] Error submitting packet to decoder: Invalid data found when processing input"]);
        assert_eq!(ffmpeg_reason(&broken, Some(183), Some(Path::new("/p/p15.jpg")), &[]), "깨진 사진이 있음: p15.jpg 근처");
    }

    #[test]
    fn unknown_ffmpeg_failure_shows_the_code() {
        assert_eq!(ffmpeg_reason(&lines(&["Conversion failed!"]), Some(234), None, &[]), "ffmpeg 오류 (코드 234)");
        assert_eq!(ffmpeg_reason(&[], None, None, &[]), "ffmpeg 오류 (코드 없음)");
    }

    #[test]
    fn output_file_errors_get_plain_words() {
        assert_eq!(io_reason(&io::ErrorKind::StorageFull.into()), disk_full());
        assert_eq!(io_reason(&io::ErrorKind::PermissionDenied.into()), no_write());
        assert_eq!(io_reason(&io::ErrorKind::NotFound.into()), "출력 폴더가 없음");
    }

    /// LAPSEWRIGHT_FFMPEG=/opt/homebrew/bin/ffmpeg cargo test -p lapsewright-app -- --ignored
    #[test]
    #[ignore = "needs ffmpeg"]
    fn converts_a_folder_and_names_the_broken_photo() {
        let ffmpeg = PathBuf::from(std::env::var_os("LAPSEWRIGHT_FFMPEG").expect("LAPSEWRIGHT_FFMPEG"));
        let root = std::env::temp_dir().join(format!("lapsewright-app-convert-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let photos = root.join("day");
        fs::create_dir_all(&photos).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gps.jpg");
        for i in 1..=12 {
            fs::copy(&fixture, photos.join(format!("p{i:02}.jpg"))).unwrap();
        }
        let settings = lapsewright_core::settings::Settings { resolution: lapsewright_core::settings::Resolution::new(320, 180).unwrap(), ..Default::default() };
        let job = Job { folder: photos.clone(), settings: settings.into(), output: root.clone(), crop: settings.crop };

        let mut events = Vec::new();
        let done = run(&ffmpeg, &job, &Prefs::default(), drop, |e| events.push(e)).expect("converted");
        assert_eq!(done.file, root.join("day.mp4"));
        assert!(done.size > 0 && done.key_error.is_none());
        assert!(matches!(&events[0], Event::Started { frames: 12, command, .. } if command.starts_with("ffmpeg -hide_banner")));
        assert!(matches!(events.last(), Some(Event::Progress { frame: 12, .. })));

        fs::write(photos.join("p07.jpg"), b"not a photo").unwrap();
        assert_eq!(run(&ffmpeg, &job, &Prefs::default(), drop, drop).expect_err("not a photo").message, "사진을 못 읽음: p07.jpg");
        fs::write(photos.join("p07.jpg"), b"").unwrap();
        assert_eq!(run(&ffmpeg, &job, &Prefs::default(), drop, drop).expect_err("empty photo").message, "사진을 못 읽음: p07.jpg");

        // Starts like a JPEG, so only the decoder finds out.
        fs::write(photos.join("p07.jpg"), b"\xff\xd8\xffnot a photo").unwrap();
        let failed = run(&ffmpeg, &job, &Prefs::default(), drop, drop).expect_err("broken photo");
        assert_eq!(failed.message, "깨진 사진이 있음: p07.jpg 근처");
        assert!(!failed.log.is_empty());
        let unblurred = Job { settings: lapsewright_core::settings::Settings { blur: lapsewright_core::settings::MotionBlur::Off, ..settings }.into(), ..job };
        assert_eq!(run(&ffmpeg, &unblurred, &Prefs::default(), drop, drop).expect_err("broken photo").message, "깨진 사진이 있음: p07.jpg 근처");
        assert!(!root.join("day_2.mp4").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn target_shows_the_next_free_name() {
        let dir = std::env::temp_dir().join(format!("lapsewright-target-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("day.mp4"), b"").unwrap();
        let target = target(&dir, "day", "mp4");
        assert_eq!((target.problem, target.file.as_deref()), (None, Some("day_2.mp4")));
        assert!(target.free.is_some_and(|f| f > 0));
        let missing = super::target(&dir.join("gone"), "day", "mp4");
        assert_eq!((missing.problem.as_deref(), missing.file), (Some("폴더가 없음"), None));
        fs::remove_dir_all(&dir).unwrap();
    }
}
