use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lapsewright_core::rules;
use lapsewright_core::settings::Settings;
use lapsewright_core::store::SettingsFile;
use serde::{Deserialize, Serialize};

use crate::lang::t;

const SESSION_FILE: &str = "last_session.json";
const SESSION_VERSION: u32 = 1;
const DEFAULT_PRESET: &str = "stock_4k_sdr";

/// What the settings panel shows; comes back on the next launch (R8).
#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    /// The preset the values came from; they may have been changed since.
    pub preset: Option<String>,
    pub settings: SettingsFile,
    /// Not part of presets (R9). `None`: next to the photo folder (C10).
    pub output: Option<PathBuf>,
}

impl Default for Session {
    fn default() -> Self {
        Session { preset: Some(DEFAULT_PRESET.to_string()), settings: Settings::default().into(), output: None }
    }
}

impl Session {
    /// The settings as the encoder will use them, written back so the panel shows what changed.
    pub fn normalize(self) -> Result<(Session, Settings), String> {
        let settings = rules::normalize(self.settings.to_settings()?);
        Ok((Session { settings: settings.into(), ..self }, settings))
    }
}

#[derive(Serialize, Deserialize)]
struct SessionFile<S> {
    version: u32,
    #[serde(flatten)]
    session: S,
}

/// A missing, unreadable or invalid file is the default: it only saves a few clicks.
pub fn load(root: &Path) -> (Session, Settings) {
    fs::read_to_string(root.join(SESSION_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<SessionFile<Session>>(&text).ok())
        .filter(|file| file.version == SESSION_VERSION)
        .and_then(|file| file.session.normalize().ok())
        .unwrap_or_else(|| Session::default().normalize().expect("default settings are valid"))
}

pub fn save(root: &Path, session: &Session) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let text = serde_json::to_string_pretty(&SessionFile { version: SESSION_VERSION, session })?;
    fs::write(root.join(SESSION_FILE), text)
}

/// C10: why no video can be written into `dir`. Only asks; never writes there.
pub fn output_problem(dir: &Path) -> Option<String> {
    if !dir.is_dir() {
        return Some(t("폴더가 없음", "The folder is missing").to_string());
    }
    if !can_write(dir) {
        return Some(t("쓸 수 없는 폴더 (읽기 전용이거나 권한 없음)", "Can’t write here (read-only or no permission)").to_string());
    }
    None
}

/// `access` asks the kernel, so read-only volumes and ACLs count too.
#[cfg(unix)]
fn can_write(dir: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: `path` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn can_write(dir: &Path) -> bool {
    fs::metadata(dir).is_ok_and(|m| !m.permissions().readonly())
}

/// B3: bytes a new file in `dir` can still take.
#[cfg(unix)]
pub fn free_space(dir: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stat` is written in full when the call returns 0.
    let stat = unsafe {
        if libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    Some(u64::from(stat.f_bavail) * u64::from(stat.f_frsize))
}

// ponytail: Windows shows no free space until GetDiskFreeSpaceExW is wired up.
#[cfg(not(unix))]
pub fn free_space(_dir: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lapsewright_core::settings::{Chroma, Codec, DynamicRange};
    use std::env;

    fn temp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lapsewright-session-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saved_session_comes_back_normalized() {
        let root = temp("roundtrip");
        // high cannot do 4:2:2 or HDR: the file keeps what the encoder would use.
        let settings = SettingsFile { chroma: Chroma::Yuv422, range: DynamicRange::Hdr, crf: 30, ..Settings::default().into() };
        let session = Session { preset: None, settings, output: Some("/out".into()) };
        let (session, _) = session.normalize().unwrap();
        save(&root, &session).unwrap();

        let (loaded, settings) = load(&root);
        assert_eq!((loaded.preset, loaded.output), (None, Some(PathBuf::from("/out"))));
        assert_eq!((loaded.settings.codec, loaded.settings.chroma, loaded.settings.range), (Codec::H264, Chroma::Yuv420, DynamicRange::Sdr));
        assert_eq!(settings.crf, 30);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_broken_or_invalid_file_is_the_default() {
        let root = temp("broken");
        let is_default = |root: &Path| {
            let (session, settings) = load(root);
            session.preset.as_deref() == Some(DEFAULT_PRESET) && session.output.is_none() && settings == Settings::default()
        };
        assert!(is_default(&root));
        fs::write(root.join(SESSION_FILE), "{").unwrap();
        assert!(is_default(&root));
        fs::write(root.join(SESSION_FILE), r#"{ "version": 1, "preset": null, "output": null, "settings": { "width": 1921 } }"#).unwrap();
        assert!(is_default(&root));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_folder_is_a_problem() {
        let root = temp("output");
        assert_eq!(output_problem(&root), None);
        assert_eq!(output_problem(&root.join("gone")).as_deref(), Some("폴더가 없음"));
        fs::remove_dir_all(&root).unwrap();
    }
}
